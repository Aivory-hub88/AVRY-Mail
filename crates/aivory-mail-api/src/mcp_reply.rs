//! MCP `get_message`, `reply_mail` and `forward_mail` (capability mode v2).
//!
//! Reply and forward derive the whole outgoing payload (recipients, subject,
//! thread, In-Reply-To, quoted body, attachments) from a stored message, so an
//! agent never has to guess thread ids or re-type recipients. They keep the
//! same two-step confirmation as `send_mail`: the first call (no
//! `confirmation_id`) returns a preview plus a fresh confirmation; the second
//! call with that id re-derives the identical payload and dispatches it.

use std::sync::Arc;

use aivory_mail_core::types::{SendAttachment, SendRequest};
use aivory_mail_storage::db::DbPool;
use axum::http::StatusCode;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::api::{execution_context::ExecutionContext, AppState};
use crate::mcp_limits::{self, McpLimits};

/// Why a tool call did not produce a normal result.
pub enum ToolFailure {
    /// Bare HTTP failure (bad arguments, storage error).
    Status(StatusCode),
    /// Readable failure delivered as tool content so the agent can act on it.
    Message(String),
    /// JSON-RPC level error (code, message).
    Rpc(i64, &'static str),
}

impl From<StatusCode> for ToolFailure {
    fn from(status: StatusCode) -> Self {
        ToolFailure::Status(status)
    }
}

fn msg<T>(text: impl Into<String>) -> Result<T, ToolFailure> {
    Err(ToolFailure::Message(text.into()))
}

pub fn tool_error(text: &str) -> Value {
    json!({"content": [{"type": "text", "text": json!({"error": text}).to_string()}]})
}

pub fn tool_text(value: Value) -> Value {
    json!({"content": [{"type": "text", "text": value.to_string()}]})
}

#[derive(Debug, Clone)]
pub struct AttachmentRef {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub r2_key: String,
}

#[derive(Debug, Clone)]
pub struct SourceMessage {
    pub id: Uuid,
    pub thread_id: Option<Uuid>,
    pub message_id: String,
    pub folder: String,
    pub from_addr: String,
    pub from_name: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body_text: String,
    pub created_at: String,
}

// ───────────────────────── pure helpers ─────────────────────────

/// `"Name <a@b.com>"` → `a@b.com` (lowercased); bare addresses pass through.
pub fn bare_address(raw: &str) -> String {
    let trimmed = raw.trim();
    let inner = match (trimmed.rfind('<'), trimmed.rfind('>')) {
        (Some(open), Some(close)) if open < close => &trimmed[open + 1..close],
        _ => trimmed,
    };
    inner.trim().to_lowercase()
}

/// Recipient columns hold a JSON array; tolerate a comma separated string.
pub fn parse_addr_column(raw: &str) -> Vec<String> {
    if let Ok(list) = serde_json::from_str::<Vec<String>>(raw) {
        return list
            .iter()
            .map(|s| bare_address(s))
            .filter(|s| !s.is_empty())
            .collect();
    }
    raw.split(',')
        .map(bare_address)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Optional string-array argument: trimmed, lowercased, blanks and duplicates dropped.
pub fn addr_list_arg(args: &Value, key: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    args.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(bare_address)
                .filter(|s| !s.is_empty() && seen.insert(s.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn prefixed_subject(subject: &str, prefix: &str) -> String {
    let trimmed = subject.trim();
    if trimmed.to_lowercase().starts_with(&prefix.to_lowercase()) {
        trimmed.to_string()
    } else if trimmed.is_empty() {
        format!("{} (no subject)", prefix)
    } else {
        format!("{} {}", prefix, trimmed)
    }
}

fn dedup_excluding(list: Vec<String>, exclude: &[&str]) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> =
        exclude.iter().map(|s| s.to_string()).collect();
    list.into_iter().filter(|a| seen.insert(a.clone())).collect()
}

fn quote_block(body: &str) -> String {
    body.lines()
        .map(|line| format!("> {}", line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Decode the HTML entities `strip_tags` leaves behind (`&amp;`, `&lt;`,
/// numeric `&#38;` / `&#x26;`, ...) back to plain characters. Without this,
/// stripping tags from `<p>CEO &amp; FOUNDER</p>` (correct HTML for a
/// literal "&") left the literal text "CEO &amp; FOUNDER" in an agent's
/// quoted reply — same bug, same fix, as the web client's sigToText.
fn decode_entities(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'&' {
            // Safe: we only ever step by whole UTF-8 chars below.
            let start = i;
            while i < bytes.len() && bytes[i] != b'&' {
                i += utf8_char_len(bytes[i]);
            }
            out.push_str(&s[start..i]);
            continue;
        }
        if let Some(end) = s[i..].find(';').map(|p| i + p) {
            let body = &s[i + 1..end];
            let decoded = match body {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\x27'),
                "nbsp" => Some(' '),
                _ if body.starts_with("#x") || body.starts_with("#X") => {
                    u32::from_str_radix(&body[2..], 16).ok().and_then(char::from_u32)
                }
                _ if body.starts_with('#') => body[1..].parse::<u32>().ok().and_then(char::from_u32),
                _ => None,
            };
            if let Some(c) = decoded {
                out.push(c);
                i = end + 1;
                continue;
            }
        }
        // Not a recognized entity: keep the '&' literally and move on.
        out.push('&');
        i += 1;
    }
    out
}

fn utf8_char_len(b: u8) -> usize {
    if b & 0x80 == 0 { 1 } else if b & 0xE0 == 0xC0 { 2 } else if b & 0xF0 == 0xE0 { 3 } else { 4 }
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    decode_entities(&out)
}

fn sender_label(src: &SourceMessage) -> String {
    match src.from_name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => format!("{} <{}>", name, bare_address(&src.from_addr)),
        None => bare_address(&src.from_addr),
    }
}

/// Build the reply payload. `own` is the (lowercase) address of the replying
/// mailbox; it never ends up as a recipient.
pub fn build_reply(
    src: &SourceMessage,
    own: &str,
    reply_all: bool,
    text: &str,
    html: Option<String>,
    extra_cc: Vec<String>,
    bcc: Vec<String>,
) -> Result<SendRequest, ToolFailure> {
    let sender = bare_address(&src.from_addr);
    // Replying to something we sent ourselves means writing to its recipients.
    let mut to = if sender == own { src.to.clone() } else { vec![sender.clone()] };
    to = dedup_excluding(to, &[own]);
    let mut cc: Vec<String> = Vec::new();
    if reply_all {
        cc.extend(src.to.iter().cloned());
        cc.extend(src.cc.iter().cloned());
        if sender != own {
            // the original sender is already in `to`
            cc.retain(|a| a != &sender);
        }
    }
    cc.extend(extra_cc);
    let mut taken: Vec<&str> = vec![own];
    taken.extend(to.iter().map(String::as_str));
    let cc = dedup_excluding(cc.clone(), &taken);
    if to.is_empty() {
        return msg("could not work out who to reply to — the original message has no other recipient; use send_mail with explicit `to`.");
    }
    let mut taken_bcc: Vec<&str> = vec![own];
    taken_bcc.extend(to.iter().map(String::as_str));
    taken_bcc.extend(cc.iter().map(String::as_str));
    let bcc = dedup_excluding(bcc, &taken_bcc);
    let quoted = format!(
        "{}\n\nOn {}, {} wrote:\n{}",
        text.trim_end(),
        src.created_at,
        sender_label(src),
        quote_block(&src.body_text)
    );
    Ok(SendRequest {
        from: own.to_string(),
        to,
        cc: (!cc.is_empty()).then_some(cc),
        bcc: (!bcc.is_empty()).then_some(bcc),
        subject: prefixed_subject(&src.subject, "Re:"),
        text: Some(quoted),
        html,
        attachments: None,
        thread_id: src.thread_id,
        in_reply_to: Some(src.message_id.clone()).filter(|s| !s.trim().is_empty()),
    })
}

/// Build the forward payload (a new conversation, original quoted in the body).
pub fn build_forward(
    src: &SourceMessage,
    own: &str,
    to: Vec<String>,
    cc: Vec<String>,
    bcc: Vec<String>,
    note: &str,
    html: Option<String>,
    attachments: Vec<SendAttachment>,
) -> Result<SendRequest, ToolFailure> {
    let to = dedup_excluding(to, &[own]);
    if to.is_empty() {
        return msg("forward_mail needs at least one recipient in `to`.");
    }
    let mut taken: Vec<&str> = vec![own];
    taken.extend(to.iter().map(String::as_str));
    let cc = dedup_excluding(cc, &taken);
    taken.extend(cc.iter().map(String::as_str));
    let bcc = dedup_excluding(bcc, &taken);
    let header = format!(
        "---------- Forwarded message ----------\nFrom: {}\nDate: {}\nSubject: {}\nTo: {}{}",
        sender_label(src),
        src.created_at,
        src.subject,
        src.to.join(", "),
        if src.cc.is_empty() { String::new() } else { format!("\nCc: {}", src.cc.join(", ")) },
    );
    let text = if note.trim().is_empty() {
        format!("{}\n\n{}", header, src.body_text)
    } else {
        format!("{}\n\n{}\n\n{}", note.trim_end(), header, src.body_text)
    };
    Ok(SendRequest {
        from: own.to_string(),
        to,
        cc: (!cc.is_empty()).then_some(cc),
        bcc: (!bcc.is_empty()).then_some(bcc),
        subject: prefixed_subject(&src.subject, "Fwd:"),
        text: Some(text),
        html,
        attachments: (!attachments.is_empty()).then_some(attachments),
        thread_id: None,
        in_reply_to: None,
    })
}

// ───────────────────────── storage ─────────────────────────

/// Load one message, scoped to the capability's tenant AND mailbox.
pub async fn load_source(
    state: &Arc<AppState>,
    context: &ExecutionContext,
    message_id: Uuid,
) -> Result<Option<SourceMessage>, StatusCode> {
    let row = match &state.db {
        DbPool::Postgres(pool) => sqlx::query(
            "SELECT id::text AS id, thread_id::text AS thread_id, message_id, folder, from_addr, from_name, to_addrs, cc_addrs, subject, body_text, body_html, created_at::text AS created_at FROM messages WHERE id=$1 AND tenant_id=$2 AND mailbox_id=$3",
        )
        .bind(message_id)
        .bind(context.tenant_id)
        .bind(context.mailbox_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(|r| SqlRow::Pg(Box::new(r))),
        DbPool::Sqlite(pool) => sqlx::query(
            "SELECT id, thread_id, message_id, folder, from_addr, from_name, to_addrs, cc_addrs, subject, body_text, body_html, created_at FROM messages WHERE id=? AND tenant_id=? AND mailbox_id=?",
        )
        .bind(message_id.to_string())
        .bind(context.tenant_id.to_string())
        .bind(context.mailbox_id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(|r| SqlRow::Sqlite(Box::new(r))),
    };
    let Some(row) = row else { return Ok(None) };
    macro_rules! col {
        ($name:expr, $ty:ty) => {
            match &row {
                SqlRow::Pg(r) => r.try_get::<$ty, _>($name).ok(),
                SqlRow::Sqlite(r) => r.try_get::<$ty, _>($name).ok(),
            }
        };
    }
    let body_text = col!("body_text", Option<String>)
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            col!("body_html", Option<String>)
                .flatten()
                .map(|h| strip_tags(&h))
        })
        .unwrap_or_default();
    Ok(Some(SourceMessage {
        id: message_id,
        thread_id: col!("thread_id", Option<String>)
            .flatten()
            .and_then(|s| Uuid::parse_str(&s).ok()),
        message_id: col!("message_id", String).unwrap_or_default(),
        folder: col!("folder", String).unwrap_or_default(),
        from_addr: col!("from_addr", String).unwrap_or_default(),
        from_name: col!("from_name", Option<String>).flatten(),
        to: parse_addr_column(&col!("to_addrs", String).unwrap_or_default()),
        cc: parse_addr_column(&col!("cc_addrs", String).unwrap_or_default()),
        subject: mcp_limits::sanitize_for_ai(&col!("subject", Option<String>).flatten().unwrap_or_default()),
        body_text: mcp_limits::sanitize_for_ai(&body_text),
        created_at: col!("created_at", String).unwrap_or_default(),
    }))
}

enum SqlRow {
    Pg(Box<sqlx::postgres::PgRow>),
    Sqlite(Box<sqlx::sqlite::SqliteRow>),
}

pub async fn load_attachments(
    state: &Arc<AppState>,
    message_id: Uuid,
) -> Result<Vec<AttachmentRef>, StatusCode> {
    let fail = |_| StatusCode::INTERNAL_SERVER_ERROR;
    Ok(match &state.db {
        DbPool::Postgres(pool) => sqlx::query(
            "SELECT id::text AS id, filename, content_type, size_bytes::bigint AS size_bytes, r2_key FROM attachments WHERE message_id=$1 ORDER BY filename",
        )
        .bind(message_id)
        .fetch_all(pool)
        .await
        .map_err(fail)?
        .into_iter()
        .map(|r| AttachmentRef {
            id: r.get("id"),
            filename: r.get("filename"),
            content_type: r.get("content_type"),
            size_bytes: r.get("size_bytes"),
            r2_key: r.get("r2_key"),
        })
        .collect(),
        DbPool::Sqlite(pool) => sqlx::query(
            "SELECT id, filename, content_type, size_bytes, r2_key FROM attachments WHERE message_id=? ORDER BY filename",
        )
        .bind(message_id.to_string())
        .fetch_all(pool)
        .await
        .map_err(fail)?
        .into_iter()
        .map(|r| AttachmentRef {
            id: r.get("id"),
            filename: r.get("filename"),
            content_type: r.get("content_type"),
            size_bytes: r.get("size_bytes"),
            r2_key: r.get("r2_key"),
        })
        .collect(),
    })
}

async fn mailbox_address(state: &Arc<AppState>, mailbox_id: Uuid) -> Result<String, ToolFailure> {
    let found = match &state.db {
        DbPool::Postgres(pool) => sqlx::query("SELECT address FROM mailboxes WHERE id=$1")
            .bind(mailbox_id)
            .fetch_optional(pool)
            .await
            .map(|r| r.map(|v| v.get::<String, _>("address"))),
        DbPool::Sqlite(pool) => sqlx::query("SELECT address FROM mailboxes WHERE id=?")
            .bind(mailbox_id.to_string())
            .fetch_optional(pool)
            .await
            .map(|r| r.map(|v| v.get::<String, _>("address"))),
    }
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    found
        .map(|a| a.trim().to_lowercase())
        .ok_or(ToolFailure::Status(StatusCode::NOT_FOUND))
}

fn message_id_arg(args: &Value) -> Result<Uuid, ToolFailure> {
    args.get("message_id")
        .and_then(Value::as_str)
        .and_then(|v| Uuid::parse_str(v.trim()).ok())
        .ok_or_else(|| {
            ToolFailure::Message(
                "message_id is required and must be a message id from search_mail.".into(),
            )
        })
}

fn optional_body(args: &Value, key: &str) -> Result<Option<String>, ToolFailure> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(Value::String(s)) if s.len() <= McpLimits::MAX_BODY_BYTES => Ok(Some(s.clone())),
        _ => msg(format!("`{}` must be a string of at most 2 MB.", key)),
    }
}

// ───────────────────────── tool handlers ─────────────────────────

pub async fn handle_get_message(
    state: &Arc<AppState>,
    context: &ExecutionContext,
    args: &Value,
) -> Result<Value, ToolFailure> {
    context.require_scope("mail.read")?;
    let id = message_id_arg(args)?;
    let Some(src) = load_source(state, context, id).await? else {
        return msg("message not found in this mailbox.");
    };
    let attachments = load_attachments(state, id).await?;
    let budget = McpLimits::MAX_THREAD_FIELD_BYTES;
    let mut body = src.body_text.clone();
    let truncated = body.len() > budget;
    if truncated {
        let mut cut = budget;
        while !body.is_char_boundary(cut) {
            cut -= 1;
        }
        body.truncate(cut);
    }
    Ok(tool_text(json!({
        "id": src.id.to_string(),
        "thread_id": src.thread_id.map(|t| t.to_string()),
        "message_id": src.message_id,
        "folder": src.folder,
        "from": src.from_addr,
        "from_name": src.from_name,
        "to": src.to,
        "cc": src.cc,
        "subject": src.subject,
        "created_at": src.created_at,
        "body_text": body,
        "body_truncated": truncated,
        "attachments": attachments.iter().map(|a| json!({
            "id": a.id, "filename": a.filename, "content_type": a.content_type, "size_bytes": a.size_bytes
        })).collect::<Vec<_>>(),
    })))
}

async fn attachments_for_forward(
    state: &Arc<AppState>,
    refs: &[AttachmentRef],
) -> Result<Vec<SendAttachment>, ToolFailure> {
    if refs.len() > McpLimits::MAX_ATTACHMENTS {
        return msg("the message has too many attachments to forward; forward with include_attachments=false.");
    }
    let mut total = 0usize;
    let mut out = Vec::with_capacity(refs.len());
    for r in refs {
        let data = state
            .store
            .get(&r.r2_key)
            .await
            .map_err(|_| ToolFailure::Message(format!("attachment {} could not be read from storage.", r.filename)))?;
        total += data.len();
        if total > 20 * 1024 * 1024 {
            return msg("attachments exceed the 20 MB send limit; forward with include_attachments=false.");
        }
        out.push(SendAttachment {
            filename: r.filename.clone(),
            content_type: Some(r.content_type.clone()),
            content_base64: B64.encode(data),
        });
    }
    Ok(out)
}

/// `reply_mail` (is_reply) and `forward_mail` share the confirmation flow.
pub async fn handle_reply_forward(
    state: &Arc<AppState>,
    context: &ExecutionContext,
    args: &Value,
    is_reply: bool,
) -> Result<Value, ToolFailure> {
    context.require_scope("mail.send")?;
    let id = message_id_arg(args)?;
    let Some(src) = load_source(state, context, id).await? else {
        return msg("message not found in this mailbox.");
    };
    let own = mailbox_address(state, context.mailbox_id).await?;
    let cc = addr_list_arg(args, "cc");
    let bcc = addr_list_arg(args, "bcc");
    let text = optional_body(args, "text")?;
    let html = optional_body(args, "html")?;

    let payload = if is_reply {
        let Some(text) = text else {
            return msg("reply_mail needs `text` — the reply you want to write.");
        };
        let reply_all = args.get("reply_all").and_then(Value::as_bool).unwrap_or(false);
        build_reply(&src, &own, reply_all, &text, html, cc, bcc)?
    } else {
        let include = args
            .get("include_attachments")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let attachments = if include {
            let refs = load_attachments(state, id).await?;
            attachments_for_forward(state, &refs).await?
        } else {
            Vec::new()
        };
        build_forward(
            &src,
            &own,
            addr_list_arg(args, "to"),
            cc,
            bcc,
            text.as_deref().unwrap_or(""),
            html,
            attachments,
        )?
    };

    aivory_mail_core::routing::validate_send_request(&payload)
        .map_err(|e| ToolFailure::Message(format!("invalid message: {}", e)))?;
    mcp_limits::validate_send_request(&payload)?;

    let confirmation_id = args
        .get("confirmation_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let Some(confirmation_id) = confirmation_id else {
        let confirmation = crate::api::mcp_confirmations::issue_send_confirmation(
            state,
            crate::api::mcp_confirmations::IssueSendConfirmationRequest {
                tenant_id: context.tenant_id.to_string(),
                mailbox_id: context.mailbox_id.to_string(),
                caller_id: context.caller_id.clone(),
                payload: payload.clone(),
                expires_in_seconds: None,
            },
        )
        .await?;
        let preview: String = payload.text.as_deref().unwrap_or("").chars().take(600).collect();
        return Ok(tool_text(json!({
            "status": "confirmation_required",
            "confirmation_id": confirmation.id,
            "expires_at": confirmation.expires_at,
            "will_send": {
                "from": payload.from,
                "to": payload.to,
                "cc": payload.cc.clone().unwrap_or_default(),
                "bcc": payload.bcc.clone().unwrap_or_default(),
                "subject": payload.subject,
                "in_reply_to_thread": payload.thread_id.map(|t| t.to_string()),
                "attachments": payload.attachments.as_ref().map(|a| a.iter().map(|x| x.filename.clone()).collect::<Vec<_>>()).unwrap_or_default(),
                "text_preview": preview,
            },
            "next": format!("Call {} again with the SAME arguments plus this confirmation_id to send.", if is_reply { "reply_mail" } else { "forward_mail" }),
        })));
    };

    crate::api::mcp_confirmations::consume_send_confirmation(state, context, confirmation_id, &payload)
        .await?;
    match crate::mail::outbound::send_email_with_context(state, payload, context).await {
        Ok(message_id) => Ok(tool_text(json!({
            "status": "succeeded",
            "message_id": message_id,
        }))),
        Err(_) => Err(ToolFailure::Rpc(
            -32011,
            "send outcome is unknown; reconcile before retrying",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(from: &str, to: &[&str], cc: &[&str]) -> SourceMessage {
        SourceMessage {
            id: Uuid::nil(),
            thread_id: Some(Uuid::nil()),
            message_id: "<abc@x>".into(),
            folder: "Inbox".into(),
            from_addr: from.into(),
            from_name: Some("Sender".into()),
            to: to.iter().map(|s| s.to_string()).collect(),
            cc: cc.iter().map(|s| s.to_string()).collect(),
            subject: "Hello".into(),
            body_text: "line1\nline2".into(),
            created_at: "2026-09-20T01:00:00Z".into(),
        }
    }

    fn ok<T>(r: Result<T, ToolFailure>) -> T {
        match r {
            Ok(v) => v,
            Err(_) => panic!("expected Ok"),
        }
    }

    #[test]
    fn bare_address_strips_display_name() {
        assert_eq!(bare_address("Ann <ANN@X.com>"), "ann@x.com");
        assert_eq!(bare_address(" b@x.com "), "b@x.com");
        assert_eq!(parse_addr_column("[\"A <a@x.com>\",\"b@x.com\"]"), vec!["a@x.com", "b@x.com"]);
        assert_eq!(parse_addr_column("a@x.com, b@x.com"), vec!["a@x.com", "b@x.com"]);
    }

    #[test]
    fn reply_goes_to_sender_and_threads() {
        let s = src("them@x.com", &["lex@aivory.uk"], &[]);
        let r = ok(build_reply(&s, "lex@aivory.uk", false, "thanks", None, vec![], vec![]));
        assert_eq!(r.to, vec!["them@x.com"]);
        assert!(r.cc.is_none());
        assert_eq!(r.subject, "Re: Hello");
        assert_eq!(r.thread_id, Some(Uuid::nil()));
        assert_eq!(r.in_reply_to.as_deref(), Some("<abc@x>"));
        assert!(r.text.unwrap().contains("> line1\n> line2"));
    }

    #[test]
    fn reply_all_keeps_others_but_never_self_or_duplicates() {
        let s = src("them@x.com", &["lex@aivory.uk", "b@x.com"], &["c@x.com", "them@x.com"]);
        let r = ok(build_reply(&s, "lex@aivory.uk", true, "ok", None, vec!["c@x.com".into(), "me@aivory.uk".into()], vec![]));
        assert_eq!(r.to, vec!["them@x.com"]);
        assert_eq!(r.cc.unwrap(), vec!["b@x.com", "c@x.com", "me@aivory.uk"]);
    }

    #[test]
    fn reply_to_own_sent_message_targets_its_recipients() {
        let s = src("lex@aivory.uk", &["them@x.com"], &["boss@x.com"]);
        let r = ok(build_reply(&s, "lex@aivory.uk", true, "ping", None, vec![], vec![]));
        assert_eq!(r.to, vec!["them@x.com"]);
        assert_eq!(r.cc.unwrap(), vec!["boss@x.com"]);
    }

    #[test]
    fn reply_bcc_is_deduplicated_against_visible_recipients() {
        let s = src("them@x.com", &["lex@aivory.uk"], &[]);
        let r = ok(build_reply(&s, "lex@aivory.uk", false, "hi", None, vec![], vec!["them@x.com".into(), "audit@x.com".into()]));
        assert_eq!(r.bcc.unwrap(), vec!["audit@x.com"]);
    }

    #[test]
    fn subject_prefix_is_not_doubled() {
        assert_eq!(prefixed_subject("Re: Hello", "Re:"), "Re: Hello");
        assert_eq!(prefixed_subject("RE: Hello", "Re:"), "RE: Hello");
        assert_eq!(prefixed_subject("Hello", "Fwd:"), "Fwd: Hello");
    }

    #[test]
    fn forward_is_a_new_thread_with_quoted_original() {
        let s = src("them@x.com", &["lex@aivory.uk"], &[]);
        let f = ok(build_forward(&s, "lex@aivory.uk", vec!["boss@x.com".into()], vec!["irfan@x.com".into()], vec![], "FYI", None, vec![]));
        assert_eq!(f.to, vec!["boss@x.com"]);
        assert_eq!(f.cc.unwrap(), vec!["irfan@x.com"]);
        assert_eq!(f.subject, "Fwd: Hello");
        assert!(f.thread_id.is_none() && f.in_reply_to.is_none());
        let t = f.text.unwrap();
        assert!(t.starts_with("FYI\n\n---------- Forwarded message ----------"));
        assert!(t.contains("From: Sender <them@x.com>") && t.ends_with("line1\nline2"));
    }

    #[test]
    fn forward_without_recipient_is_rejected() {
        let s = src("them@x.com", &["lex@aivory.uk"], &[]);
        assert!(build_forward(&s, "lex@aivory.uk", vec![], vec![], vec![], "", None, vec![]).is_err());
    }

    #[test]
    fn same_inputs_produce_identical_payload_hash() {
        let s = src("them@x.com", &["lex@aivory.uk"], &[]);
        let a = ok(build_reply(&s, "lex@aivory.uk", true, "same", None, vec![], vec![]));
        let b = ok(build_reply(&s, "lex@aivory.uk", true, "same", None, vec![], vec![]));
        assert_eq!(
            crate::api::mcp_confirmations::payload_hash(&a).ok(),
            crate::api::mcp_confirmations::payload_hash(&b).ok()
        );
    }

    #[test]
    fn strip_tags_decodes_amp_the_reported_bug() {
        // What an HTML-only message body looks like for a literal "&" in a
        // signature: correct HTML, and this is the only source an agent's
        // quoted reply has when body_text is empty.
        assert_eq!(
            strip_tags("<p>CEO &amp; FOUNDER</p>"),
            "CEO & FOUNDER"
        );
    }

    #[test]
    fn strip_tags_decodes_other_entities_and_leaves_plain_amp_alone() {
        assert_eq!(strip_tags("a &lt;tag&gt; &amp; &quot;q&quot; &amp; it&apos;s&nbsp;fine"), "a <tag> & \"q\" & it's fine");
        assert_eq!(strip_tags("&#38; and &#x26;"), "& and &");
        assert_eq!(strip_tags("Fish & Chips"), "Fish & Chips");
        assert_eq!(strip_tags("Q&amp;A &notareal; done"), "Q&A &notareal; done");
    }

    #[test]
    fn strip_tags_decoded_entity_never_reopens_a_tag() {
        // &lt;b&gt; must become the literal text "<b>", not an actual tag —
        // decode runs strictly after stripping, never re-parsed as markup.
        assert_eq!(strip_tags("before&lt;b&gt;after"), "before<b>after");
    }

    #[test]
    fn decode_entities_handles_multibyte_text_around_entities() {
        assert_eq!(decode_entities("café &amp; thé"), "café & thé");
    }
}
