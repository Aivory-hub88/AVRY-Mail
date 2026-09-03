use serde_json::Value;

/// One enabled filter row: (criteria_json, action_json) + priority.
pub struct FilterRule<'a> {
    pub criteria: &'a Value,
    pub action: &'a Value,
    pub priority: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilterAction {
    Move(String),
    Reject(String),
    Block,
    Forward(String),
    None,
}

fn matches(criteria: &Value, from: &str, subject: &str, body: &str) -> bool {
    let Some(obj) = criteria.as_object() else { return false };
    if obj.is_empty() { return false; }
    obj.iter().all(|(key, needle)| {
        let Some(needle) = needle.as_str() else { return true };
        if needle.is_empty() { return true; }
        let haystack = match key.as_str() {
            "from" => from,
            "subject" => subject,
            "body" => body,
            "to" => from, // alias: 'to' criteria checks recipient's from for inbound
            _ => return true, // unknown criteria key: don't block the match on it
        };
        haystack.to_lowercase().contains(&needle.to_lowercase())
    })
}

/// Run enabled filters in priority order, return the target folder of the first match
/// (Gmail/Zoho semantics: lowest priority wins), or None to fall back to Inbox.
pub fn resolve_folder(rules: &[FilterRule], from: &str, subject: &str, body: &str) -> Option<String> {
    // sort by priority ascending (stable)
    let mut sorted: Vec<&FilterRule> = rules.iter().collect();
    sorted.sort_by_key(|r| r.priority);
    for rule in sorted {
        if matches(rule.criteria, from, subject, body) {
            if let Some(folder) = rule.action.get("move").and_then(|v| v.as_str()) {
                return Some(folder.to_string());
            }
            if let Some(folder) = rule.action.get("folder").and_then(|v| v.as_str()) {
                return Some(folder.to_string());
            }
            if let Some(f) = rule.action.get("action").and_then(|v| v.as_str()) {
                // Mailflare-style action string
                if f.starts_with("move:") { return Some(f.trim_start_matches("move:").to_string()); }
                if f == "reject" || f == "block" { return Some("Spam".into()); }
            }
        }
    }
    None
}

pub fn resolve_action(rules: &[FilterRule], from: &str, subject: &str, body: &str) -> FilterAction {
    let mut sorted: Vec<&FilterRule> = rules.iter().collect();
    sorted.sort_by_key(|r| r.priority);
    for rule in sorted {
        if !matches(rule.criteria, from, subject, body) { continue; }
        let act = rule.action;
        if act.get("reject").and_then(|v| v.as_bool()).unwrap_or(false) {
            let reason = act.get("reason").and_then(|v| v.as_str()).unwrap_or("rejected by filter").to_string();
            return FilterAction::Reject(reason);
        }
        if act.get("block").and_then(|v| v.as_bool()).unwrap_or(false) {
            return FilterAction::Block;
        }
        if let Some(fw) = act.get("forward").and_then(|v| v.as_str()) {
            if !fw.is_empty() { return FilterAction::Forward(fw.to_string()); }
        }
        if let Some(folder) = act.get("move").and_then(|v| v.as_str()) {
            return FilterAction::Move(folder.to_string());
        }
        if let Some(folder) = act.get("folder").and_then(|v| v.as_str()) {
            return FilterAction::Move(folder.to_string());
        }
    }
    FilterAction::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_from_substring() {
        let criteria = json!({"from": "finance@"});
        let action = json!({"move": "Spam"});
        let rules = vec![FilterRule { criteria: &criteria, action: &action, priority: 0 }];
        assert_eq!(resolve_folder(&rules, "finance@acme.com", "Invoice", "body"), Some("Spam".into()));
        assert_eq!(resolve_folder(&rules, "sales@acme.com", "Invoice", "body"), None);
    }

    #[test]
    fn first_match_wins() {
        let c1 = json!({"subject": "urgent"});
        let a1 = json!({"move": "Inbox"});
        let c2 = json!({"subject": "urgent"});
        let a2 = json!({"move": "Trash"});
        let rules = vec![
            FilterRule { criteria: &c1, action: &a1, priority: 0 },
            FilterRule { criteria: &c2, action: &a2, priority: 0 },
        ];
        assert_eq!(resolve_folder(&rules, "x@y.com", "Urgent: reply now", "b"), Some("Inbox".into()));
    }

    #[test]
    fn priority_order() {
        let c1 = json!({"from": "a@"});
        let a1 = json!({"move": "Spam"});
        let c2 = json!({"from": "a@"});
        let a2 = json!({"move": "Trash"});
        let rules = vec![
            FilterRule { criteria: &c1, action: &a1, priority: 10 },
            FilterRule { criteria: &c2, action: &a2, priority: 1 },
        ];
        // lower priority wins regardless of insertion order
        assert_eq!(resolve_folder(&rules, "a@b.com", "x", "y"), Some("Trash".into()));
    }

    #[test]
    fn reject_action() {
        let c = json!({"from": "spam@"});
        let a = json!({"reject": true, "reason": "blocked"});
        let rules = vec![FilterRule { criteria: &c, action: &a, priority: 0 }];
        assert_eq!(resolve_action(&rules, "spam@evil.com", "hi", "body"), FilterAction::Reject("blocked".into()));
    }
}
