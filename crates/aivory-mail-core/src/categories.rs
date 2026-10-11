//! Inbox tabs: Primary, Promotions, Social, Updates, Forums.
//!
//! Classification reads only the envelope and headers — no message body ever
//! leaves the server or is sent to a model. The guiding rule is that mail
//! must never be hidden by a guess: **without a signal that the message was
//! sent by a machine, it stays in Primary**, whatever its subject says. An
//! invoice typed by a client lands in Primary; the same word from a billing
//! system's noreply address lands in Updates.
//!
//! A user's own choice always wins: once they move a sender's conversation to
//! a tab, that sender is locked there (`SenderLock`), and future mail follows.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Primary,
    Promotions,
    Social,
    Updates,
    Forums,
}

pub const ALL: [Category; 5] = [
    Category::Primary,
    Category::Promotions,
    Category::Social,
    Category::Updates,
    Category::Forums,
];

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Primary => "primary",
            Category::Promotions => "promotions",
            Category::Social => "social",
            Category::Updates => "updates",
            Category::Forums => "forums",
        }
    }

    pub fn parse(value: &str) -> Option<Category> {
        ALL.into_iter()
            .find(|c| c.as_str().eq_ignore_ascii_case(value.trim()))
    }

    /// Tabs whose new mail should not ring the user's phone.
    pub fn is_quiet(self) -> bool {
        matches!(self, Category::Promotions | Category::Social)
    }
}

/// Which tabs a mailbox shows, from the `inbox.categories` user setting
/// (e.g. `"Primary,Promotions,Social"`). Primary is always on; unknown names
/// are ignored. Mail in a hidden tab is shown in Primary.
pub fn enabled_from_setting(setting: &str) -> Vec<Category> {
    let mut out = vec![Category::Primary];
    for part in setting.split(',') {
        if let Some(c) = Category::parse(part) {
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out
}

/// The stored categories a tab lists: its own, plus — for Primary — every
/// category whose tab is hidden. (Unclassified rows are Primary too; callers
/// treat a NULL category as `primary`.)
pub fn stored_categories_for_tab(tab: Category, enabled: &[Category]) -> Vec<Category> {
    if tab != Category::Primary {
        return if enabled.contains(&tab) { vec![tab] } else { vec![] };
    }
    ALL.into_iter()
        .filter(|c| *c == Category::Primary || !enabled.contains(c))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Classification {
    pub category: Category,
    /// Short machine-readable reason, kept for debugging misfiles.
    pub reason: &'static str,
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn has_header(headers: &[(String, String)], name: &str) -> bool {
    header(headers, name).is_some()
}

fn has_header_prefix(headers: &[(String, String)], prefix: &str) -> bool {
    headers
        .iter()
        .any(|(k, _)| k.get(..prefix.len()).is_some_and(|head| head.eq_ignore_ascii_case(prefix)))
}

fn domain_of(addr: &str) -> String {
    addr.rsplit('@').next().unwrap_or("").trim().trim_end_matches('>').to_lowercase()
}

fn local_part_of(addr: &str) -> String {
    let addr = addr.trim().trim_start_matches('<');
    addr.split('@').next().unwrap_or("").to_lowercase()
}

/// `domain` is `base` or a subdomain of it.
fn domain_is(domain: &str, base: &str) -> bool {
    domain == base || domain.ends_with(&format!(".{}", base))
}

const SOCIAL_DOMAINS: &[&str] = &[
    "facebookmail.com",
    "facebook.com",
    "instagram.com",
    "linkedin.com",
    "twitter.com",
    "x.com",
    "tiktok.com",
    "pinterest.com",
    "threads.net",
    "youtube.com",
    "snapchat.com",
    "tumblr.com",
    "quora.com",
    "medium.com",
];

const FORUM_DOMAINS: &[&str] = &[
    "googlegroups.com",
    "groups.io",
    "discoursemail.com",
    "reddit.com",
    "redditmail.com",
    "stackexchange.com",
    "stackoverflow.email",
];

/// Local parts that only ever belong to systems, not people.
const AUTOMATED_LOCAL_PARTS: &[&str] = &[
    "noreply",
    "no-reply",
    "no_reply",
    "donotreply",
    "do-not-reply",
    "do_not_reply",
    "notification",
    "notifications",
    "notify",
    "alerts",
    "alert",
    "mailer-daemon",
    "postmaster",
    "bounce",
    "bounces",
    "automated",
    "system",
];

/// Header prefixes set by bulk/marketing senders.
const ESP_HEADER_PREFIXES: &[&str] = &[
    "x-mailchimp",
    "x-mc-user",
    "x-campaign",
    "x-mailgun",
    "x-sg-",
    "x-sendgrid",
    "x-ses-",
    "x-mailjet",
    "x-klaviyo",
    "x-hubspot",
    "x-marketo",
    "x-brevo",
    "x-sib-",
    "x-constantcontact",
    "x-emarsys",
    "x-moengage",
];

/// Subject words that mark marketing, English + Indonesian. Only consulted
/// once a message is already known to be machine-sent.
const PROMO_WORDS: &[&str] = &[
    "% off",
    "sale",
    "discount",
    "promo",
    "deal",
    "coupon",
    "voucher",
    "free shipping",
    "limited time",
    "flash sale",
    "newsletter",
    "webinar",
    "diskon",
    "potongan",
    "cashback",
    "gratis ongkir",
    "penawaran",
    "hemat",
];

fn subject_has_promo_word(subject: &str) -> bool {
    let s = subject.to_lowercase();
    PROMO_WORDS.iter().any(|w| s.contains(w))
}

/// Classify one message from its sender, subject and headers.
pub fn classify(from_addr: &str, subject: &str, headers: &[(String, String)]) -> Classification {
    let domain = domain_of(from_addr);
    let local = local_part_of(from_addr);

    let list_unsubscribe = has_header(headers, "List-Unsubscribe");
    let list_id = has_header(headers, "List-Id");
    let list_post = has_header(headers, "List-Post");
    let precedence = header(headers, "Precedence").unwrap_or("").trim().to_lowercase();
    let auto_submitted = header(headers, "Auto-Submitted")
        .map(|v| !v.trim().eq_ignore_ascii_case("no"))
        .unwrap_or(false);
    let esp = ESP_HEADER_PREFIXES.iter().any(|p| has_header_prefix(headers, p));
    let automated_sender = AUTOMATED_LOCAL_PARTS
        .iter()
        .any(|p| local == *p || local.starts_with(&format!("{}+", p)) || local.starts_with(&format!("{}.", p)));

    // Social networks notify from their own domains; a person never mails
    // you from facebookmail.com.
    if SOCIAL_DOMAINS.iter().any(|d| domain_is(&domain, d)) {
        return Classification { category: Category::Social, reason: "social_domain" };
    }

    // A real discussion list: people post to it and replies go back to it.
    if FORUM_DOMAINS.iter().any(|d| domain_is(&domain, d)) || (list_id && list_post) || precedence == "list" {
        return Classification { category: Category::Forums, reason: "mailing_list" };
    }

    let bulk = precedence == "bulk" || precedence == "junk";
    let machine_sent = list_unsubscribe || list_id || bulk || auto_submitted || esp || automated_sender;
    if !machine_sent {
        return Classification { category: Category::Primary, reason: "personal" };
    }

    // Marketing: bulk/ESP mail you can unsubscribe from, or machine mail
    // whose subject is selling something.
    if (list_unsubscribe && (bulk || esp)) || subject_has_promo_word(subject) {
        return Classification { category: Category::Promotions, reason: "marketing" };
    }

    // Everything else a machine sent: receipts, shipping, alerts, statements,
    // newsletters without a sales pitch.
    Classification { category: Category::Updates, reason: "automated" }
}

/// The final tab for a new message: the user's lock on this sender wins over
/// any rule.
pub fn decide(sender_lock: Option<Category>, rules: Classification) -> Classification {
    match sender_lock {
        Some(category) => Classification { category, reason: "sender_lock" },
        None => rules,
    }
}

/// Headers as stored in `messages.headers_json`: `[[name, value], ...]`.
pub fn headers_from_json(value: &serde_json::Value) -> Vec<(String, String)> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|pair| {
            let pair = pair.as_array()?;
            Some((pair.first()?.as_str()?.to_string(), pair.get(1)?.as_str()?.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn person_stays_primary_even_with_business_words() {
        let c = classify("budi@client.co.id", "Invoice #42 overdue — 50% off if paid today", &h(&[]));
        assert_eq!(c.category, Category::Primary);
        let c = classify("Budi <budi@client.co.id>", "Re: proposal", &h(&[("In-Reply-To", "<x@y>")]));
        assert_eq!(c.category, Category::Primary);
    }

    #[test]
    fn social_networks() {
        assert_eq!(classify("notification@facebookmail.com", "New login", &h(&[])).category, Category::Social);
        assert_eq!(classify("messages-noreply@linkedin.com", "You appeared in 9 searches", &h(&[])).category, Category::Social);
        assert_eq!(classify("info@x.com", "x", &h(&[])).category, Category::Social);
        // lookalike domain is not social
        assert_ne!(classify("a@notlinkedin.com", "hi", &h(&[])).category, Category::Social);
    }

    #[test]
    fn mailing_lists_are_forums() {
        let list = h(&[("List-Id", "<dev.lists.example.org>"), ("List-Post", "<mailto:dev@lists.example.org>")]);
        assert_eq!(classify("ann@example.org", "Re: RFC", &list).category, Category::Forums);
        assert_eq!(classify("x@googlegroups.com", "topic", &h(&[])).category, Category::Forums);
        assert_eq!(classify("ann@example.org", "t", &h(&[("Precedence", "list")])).category, Category::Forums);
    }

    #[test]
    fn marketing_is_promotions() {
        let esp = h(&[("List-Unsubscribe", "<https://u>"), ("X-Mailchimp-Campaign", "1")]);
        assert_eq!(classify("hello@shop.example", "Our autumn collection", &esp).category, Category::Promotions);
        let bulk = h(&[("List-Unsubscribe", "<https://u>"), ("Precedence", "bulk")]);
        assert_eq!(classify("news@shop.example", "This week", &bulk).category, Category::Promotions);
        let unsub = h(&[("List-Unsubscribe", "<https://u>")]);
        assert_eq!(classify("hi@shop.example", "Flash sale: gratis ongkir!", &unsub).category, Category::Promotions);
    }

    #[test]
    fn transactional_is_updates() {
        assert_eq!(classify("noreply@bank.example", "Your statement is ready", &h(&[])).category, Category::Updates);
        assert_eq!(classify("no-reply+abc@shop.example", "Order shipped", &h(&[])).category, Category::Updates);
        let auto = h(&[("Auto-Submitted", "auto-generated")]);
        assert_eq!(classify("billing@saas.example", "Receipt for your payment", &auto).category, Category::Updates);
        // a newsletter you can unsubscribe from but that isn't selling
        let unsub = h(&[("List-Unsubscribe", "<https://u>")]);
        assert_eq!(classify("digest@blog.example", "Weekly digest", &unsub).category, Category::Updates);
        // Auto-Submitted: no is a human
        assert_eq!(classify("a@b.example", "hi", &h(&[("Auto-Submitted", "no")])).category, Category::Primary);
    }

    #[test]
    fn sender_lock_wins() {
        let rules = classify("hello@shop.example", "sale", &h(&[("List-Unsubscribe", "<u>")]));
        assert_eq!(decide(Some(Category::Primary), rules.clone()).category, Category::Primary);
        assert_eq!(decide(None, rules).category, Category::Promotions);
    }

    #[test]
    fn enabled_tabs_and_primary_fallback() {
        let enabled = enabled_from_setting("Primary, Promotions,social,bogus");
        assert_eq!(enabled, vec![Category::Primary, Category::Promotions, Category::Social]);
        assert_eq!(enabled_from_setting(""), vec![Category::Primary]);
        let primary = stored_categories_for_tab(Category::Primary, &enabled);
        assert_eq!(primary, vec![Category::Primary, Category::Updates, Category::Forums]);
        assert_eq!(stored_categories_for_tab(Category::Social, &enabled), vec![Category::Social]);
        assert!(stored_categories_for_tab(Category::Updates, &enabled).is_empty());
    }

    #[test]
    fn headers_json_round_trip() {
        let v = serde_json::json!([["List-Id", "<a>"], ["Subject", "x"], ["bad"]]);
        assert_eq!(headers_from_json(&v), h(&[("List-Id", "<a>"), ("Subject", "x")]));
        assert!(headers_from_json(&serde_json::Value::Null).is_empty());
    }
}
