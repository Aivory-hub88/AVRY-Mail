//! Telegram notifications: a message to the mailbox owner's Telegram chat when
//! new mail arrives in the Inbox.
//!
//! One bot per server (`TELEGRAM_BOT_TOKEN`); each mailbox stores its own
//! `chat_id` (and two switches) in `user_settings`, category `notifications`:
//!
//! | key                | meaning                                               |
//! |--------------------|-------------------------------------------------------|
//! | `telegram_chat_id` | numeric chat id (user, or negative for a group)       |
//! | `telegram_enabled` | `"false"` turns it off; anything else = on            |
//! | `telegram_preview` | `"true"` adds the first lines of the body (default off)|
//!
//! The default message is sender + subject only: mail content goes to a third
//! party only when the owner asks for it. Promotions and Social mail never
//! notifies (same rule as web push). Delivery is fire-and-forget and never
//! blocks inbound handling.

use crate::api::{authz, AppState};
use crate::mcp_limits::sanitize_for_ai;
use aivory_mail_storage::db::DbPool;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::{json, Value};
use sqlx::Row;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

const DEFAULT_API_BASE: &str = "https://api.telegram.org";
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// Telegram allows about one message per second to the same chat.
const MIN_GAP_PER_CHAT: Duration = Duration::from_millis(1100);
/// A burst longer than this is dropped rather than queued as sleeping tasks.
const MAX_QUEUE_WAIT: Duration = Duration::from_secs(20);
const MAX_FROM_CHARS: usize = 120;
const MAX_SUBJECT_CHARS: usize = 200;
const MAX_PREVIEW_CHARS: usize = 300;
const TEST_COOLDOWN: Duration = Duration::from_secs(10);

fn api_base() -> String {
    std::env::var("TELEGRAM_API_BASE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn bot_token() -> Option<String> {
    std::env::var("TELEGRAM_BOT_TOKEN").ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

// ───────────────────────── pure helpers ─────────────────────────

/// Numeric chat ids only: digits, with a leading `-` for groups/supergroups.
/// (`@channelname` targets would let one user message any public channel
/// through the shared bot.)
pub fn valid_chat_id(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    !digits.is_empty() && digits.len() <= 20 && digits.chars().all(|c| c.is_ascii_digit())
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn clean(text: &str, max_chars: usize) -> String {
    let stripped = sanitize_for_ai(text);
    let single_line: String = stripped
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let squeezed = single_line.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = squeezed.chars().take(max_chars).collect();
    if squeezed.chars().count() > max_chars {
        out.push('…');
    }
    out
}

/// The HTML-formatted notification text.
pub fn format_message(from: &str, subject: &str, preview: Option<&str>, link: Option<&str>) -> String {
    let from = clean(from, MAX_FROM_CHARS);
    let subject = clean(subject, MAX_SUBJECT_CHARS);
    let mut out = format!(
        "<b>{}</b>\n{}",
        html_escape(if from.is_empty() { "New message" } else { &from }),
        html_escape(if subject.is_empty() { "(no subject)" } else { &subject }),
    );
    if let Some(p) = preview.map(|p| clean(p, MAX_PREVIEW_CHARS)).filter(|p| !p.is_empty()) {
        out.push_str(&format!("\n\n<i>{}</i>", html_escape(&p)));
    }
    if let Some(link) = link {
        // The link is built by the server from its own base URL and a UUID.
        out.push_str(&format!("\n\n<a href=\"{}\">Open</a>", html_escape(link)));
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
pub enum SendError {
    /// The server's bot token was rejected.
    BadBotToken,
    /// Telegram doesn't know that chat, or the bot was never started there.
    ChatNotFound,
    /// The user blocked the bot.
    Blocked,
    RateLimited,
    Other,
}

impl SendError {
    pub fn from_response(status: u16, description: &str) -> SendError {
        let d = description.to_lowercase();
        match status {
            401 | 404 if !d.contains("chat") => SendError::BadBotToken,
            403 => SendError::Blocked,
            400 if d.contains("chat not found") || d.contains("chat_id") => SendError::ChatNotFound,
            429 => SendError::RateLimited,
            _ => SendError::Other,
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            SendError::BadBotToken => "The Telegram bot token configured on this server was rejected. Ask your admin to check TELEGRAM_BOT_TOKEN.",
            SendError::ChatNotFound => "Telegram can't find that chat. Open the bot in Telegram and press Start first, then check the chat ID.",
            SendError::Blocked => "You blocked the bot in Telegram. Unblock it and press Start, then try again.",
            SendError::RateLimited => "Telegram is rate limiting this bot. Try again in a moment.",
            SendError::Other => "Telegram didn't accept the message. Try again later.",
        }
    }
}

/// How long a send to `chat` must wait to keep `gap` between messages, and
/// the slot it reserves. `slots` holds the next free instant per chat.
fn reserve_slot(slots: &mut HashMap<String, Instant>, chat: &str, now: Instant, gap: Duration) -> Duration {
    slots.retain(|_, next| *next + Duration::from_secs(60) > now);
    let start = slots.get(chat).copied().filter(|n| *n > now).unwrap_or(now);
    slots.insert(chat.to_string(), start + gap);
    start.saturating_duration_since(now)
}

fn slots() -> &'static Mutex<HashMap<String, Instant>> {
    static SLOTS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

// ───────────────────────── Telegram HTTP ─────────────────────────

/// `reqwest` errors print the request URL, and the bot token is in the URL.
/// Never log one without stripping it.
fn describe(e: reqwest::Error) -> String {
    e.without_url().to_string()
}

async fn send_text(token: &str, chat_id: &str, text: &str) -> Result<(), SendError> {
    let client = reqwest::Client::builder().timeout(SEND_TIMEOUT).build().map_err(|_| SendError::Other)?;
    let resp = client
        .post(format!("{}/bot{}/sendMessage", api_base(), token))
        .json(&json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "HTML",
            "disable_web_page_preview": true,
        }))
        .send()
        .await
        .map_err(|e| {
            tracing::warn!("telegram: request failed: {}", describe(e));
            SendError::Other
        })?;
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    let description = body["description"].as_str().unwrap_or("");
    let err = SendError::from_response(status.as_u16(), description);
    tracing::warn!("telegram: sendMessage answered {} ({:?})", status.as_u16(), err);
    Err(err)
}

async fn bot_username(token: &str) -> Option<String> {
    static CACHE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    if let Some(name) = cache.lock().ok().and_then(|c| c.clone()) {
        return Some(name);
    }
    let client = reqwest::Client::builder().timeout(SEND_TIMEOUT).build().ok()?;
    let body: Value = client
        .get(format!("{}/bot{}/getMe", api_base(), token))
        .send()
        .await
        .map_err(|e| tracing::warn!("telegram: getMe failed: {}", describe(e)))
        .ok()?
        .json()
        .await
        .ok()?;
    let name = body["result"]["username"].as_str()?.to_string();
    if let Ok(mut c) = cache.lock() {
        *c = Some(name.clone());
    }
    Some(name)
}

// ───────────────────────── settings ─────────────────────────

#[derive(Debug, Default, Clone)]
struct MailboxSettings {
    chat_id: String,
    enabled: bool,
    preview: bool,
}

async fn load_settings(state: &Arc<AppState>, mailbox_id: &str) -> MailboxSettings {
    // A mailbox-specific value wins over an instance-wide one (NULL mailbox).
    let rows: Vec<(String, String)> = match &state.db {
        DbPool::Postgres(pool) => sqlx::query("SELECT key, value FROM user_settings WHERE category='notifications' AND key IN ('telegram_chat_id','telegram_enabled','telegram_preview') AND (mailbox_id=$1 OR mailbox_id IS NULL) ORDER BY (mailbox_id IS NULL) DESC")
            .bind(mailbox_id)
            .fetch_all(pool)
            .await
            .map(|r| r.into_iter().map(|row| (row.get("key"), row.get("value"))).collect())
            .unwrap_or_default(),
        DbPool::Sqlite(pool) => sqlx::query("SELECT key, value FROM user_settings WHERE category='notifications' AND key IN ('telegram_chat_id','telegram_enabled','telegram_preview') AND (mailbox_id=? OR mailbox_id IS NULL OR mailbox_id='') ORDER BY (mailbox_id IS NULL OR mailbox_id='') DESC")
            .bind(mailbox_id)
            .fetch_all(pool)
            .await
            .map(|r| r.into_iter().map(|row| (row.get("key"), row.get("value"))).collect())
            .unwrap_or_default(),
    };
    // rows are ordered instance-wide first, mailbox-specific last, so a later
    // row for the same key overrides an earlier one.
    let mut s = MailboxSettings { enabled: true, ..Default::default() };
    for (key, value) in rows {
        let value = value.trim().to_string();
        match key.as_str() {
            "telegram_chat_id" => s.chat_id = value,
            "telegram_enabled" => s.enabled = value != "false",
            "telegram_preview" => s.preview = value == "true",
            _ => {}
        }
    }
    s
}

// ───────────────────────── inbound hook ─────────────────────────

/// Send the notification for one new Inbox message. Never returns an error:
/// failures are logged, because delivering mail must not depend on Telegram.
pub async fn notify_new_mail(
    state: Arc<AppState>,
    mailbox_id: String,
    message_id: String,
    from: String,
    subject: String,
    preview: String,
) {
    let Some(token) = bot_token() else { return };
    let settings = load_settings(&state, &mailbox_id).await;
    if !settings.enabled || settings.chat_id.is_empty() {
        return;
    }
    if !valid_chat_id(&settings.chat_id) {
        tracing::warn!("telegram: mailbox {mailbox_id} has an invalid chat id; not sending");
        return;
    }
    let wait = {
        let mut guard = match slots().lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        reserve_slot(&mut guard, &settings.chat_id, Instant::now(), MIN_GAP_PER_CHAT)
    };
    if wait > MAX_QUEUE_WAIT {
        tracing::warn!("telegram: dropping a notification for mailbox {mailbox_id}: chat is being flooded");
        return;
    }
    if !wait.is_zero() {
        tokio::time::sleep(wait).await;
    }
    let link = format!("{}/?open={}", state.config.public_base_url.trim_end_matches('/'), message_id);
    let text = format_message(
        &from,
        &subject,
        settings.preview.then_some(preview.as_str()),
        Some(&link),
    );
    if let Err(e) = send_text(&token, &settings.chat_id, &text).await {
        tracing::warn!("telegram: notification for mailbox {mailbox_id} failed: {:?}", e);
    }
}

// ───────────────────────── HTTP API ─────────────────────────

/// GET /v1/telegram/status — whether this server has a bot, and its @name so
/// the settings page can tell the user where to press Start.
pub async fn status(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    authz::authenticated_email(&state, &headers)?;
    let Some(token) = bot_token() else {
        return Ok(Json(json!({"success": true, "data": {"configured": false, "bot_username": null}})));
    };
    let username = bot_username(&token).await;
    Ok(Json(json!({"success": true, "data": {"configured": true, "bot_username": username}})))
}

/// POST /v1/telegram/test `{mailbox_id}` — send a test message to the chat
/// saved for that mailbox.
pub async fn test(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let mailbox = authz::mailbox_scope(&state, &headers, body.get("mailbox_id").and_then(Value::as_str))
        .await?
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_string();
    let fail = |status: StatusCode, message: &str| Ok((status, Json(json!({"success": false, "error": message}))));

    let Some(token) = bot_token() else {
        return fail(StatusCode::SERVICE_UNAVAILABLE, "Telegram isn't set up on this server.");
    };
    // The shared bot must not become a message cannon: one test per mailbox
    // every few seconds.
    {
        static LAST: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
        let mut last = LAST.get_or_init(|| Mutex::new(HashMap::new())).lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let now = Instant::now();
        if last.get(&mailbox).is_some_and(|t| now.duration_since(*t) < TEST_COOLDOWN) {
            return fail(StatusCode::TOO_MANY_REQUESTS, "Wait a few seconds before sending another test.");
        }
        last.insert(mailbox.clone(), now);
    }
    let settings = load_settings(&state, &mailbox).await;
    if settings.chat_id.is_empty() {
        return fail(StatusCode::BAD_REQUEST, "Save your Telegram chat ID first.");
    }
    if !valid_chat_id(&settings.chat_id) {
        return fail(StatusCode::BAD_REQUEST, "The chat ID must be a number, for example 123456789.");
    }
    let text = format_message("Aivory Mail", "Telegram notifications are connected.", None, None);
    match send_text(&token, &settings.chat_id, &text).await {
        Ok(()) => Ok((StatusCode::OK, Json(json!({"success": true})))),
        Err(e) => {
            let status = if e == SendError::BadBotToken { StatusCode::BAD_GATEWAY } else { StatusCode::BAD_REQUEST };
            fail(status, e.message())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_ids_are_numeric_only() {
        assert!(valid_chat_id("123456789"));
        assert!(valid_chat_id("-1001234567890"));
        for bad in ["", "-", "@somechannel", "12 34", "12a", "1e5", "--1", "123456789012345678901"] {
            assert!(!valid_chat_id(bad), "{bad:?}");
        }
    }

    #[test]
    fn html_in_mail_cannot_inject_markup() {
        let m = format_message("Evil <b>Corp</b> <evil@x>", "<a href=\"https://phish\">Click</a> & win", None, None);
        assert!(!m.contains("<a href=\"https://phish\">"));
        assert!(m.contains("&lt;a href=\"https://phish\"&gt;Click&lt;/a&gt; &amp; win"));
        // the only tags are the ones we add
        assert!(m.starts_with("<b>Evil &lt;b&gt;Corp&lt;/b&gt; &lt;evil@x&gt;</b>\n"));
    }

    #[test]
    fn newlines_hidden_unicode_and_length_are_handled() {
        let m = format_message("a\r\nb\u{202E}c", &"x".repeat(1000), None, None);
        assert!(m.starts_with("<b>a bc</b>\n"));
        assert!(m.chars().count() < 500);
        assert!(m.contains('…'));
    }

    #[test]
    fn empty_fields_get_placeholders() {
        assert_eq!(format_message("", "  ", None, None), "<b>New message</b>\n(no subject)");
    }

    #[test]
    fn preview_and_link_are_optional_and_escaped() {
        let m = format_message("A", "S", Some("1 < 2\nsecond line"), Some("https://mail.example/?open=1&x=2"));
        assert!(m.contains("<i>1 &lt; 2 second line</i>"));
        assert!(m.ends_with("<a href=\"https://mail.example/?open=1&amp;x=2\">Open</a>"));
        assert!(!format_message("A", "S", None, None).contains("<i>"));
    }

    #[test]
    fn telegram_errors_map_to_actionable_messages() {
        use SendError::*;
        assert_eq!(SendError::from_response(401, "Unauthorized"), BadBotToken);
        assert_eq!(SendError::from_response(400, "Bad Request: chat not found"), ChatNotFound);
        assert_eq!(SendError::from_response(403, "Forbidden: bot was blocked by the user"), Blocked);
        assert_eq!(SendError::from_response(429, "Too Many Requests: retry after 5"), RateLimited);
        assert_eq!(SendError::from_response(500, "oops"), Other);
        // a 404 about a chat is not a bad token
        assert_ne!(SendError::from_response(404, "Not Found: chat not found"), BadBotToken);
    }

    #[test]
    fn same_chat_is_spaced_and_other_chats_are_not() {
        let mut slots = HashMap::new();
        let now = Instant::now();
        let gap = Duration::from_millis(1000);
        assert_eq!(reserve_slot(&mut slots, "a", now, gap), Duration::ZERO);
        assert_eq!(reserve_slot(&mut slots, "a", now, gap), gap);
        assert_eq!(reserve_slot(&mut slots, "a", now, gap), gap * 2);
        assert_eq!(reserve_slot(&mut slots, "b", now, gap), Duration::ZERO);
        // later, once the slots have passed, no wait
        assert_eq!(reserve_slot(&mut slots, "a", now + Duration::from_secs(10), gap), Duration::ZERO);
    }
}
