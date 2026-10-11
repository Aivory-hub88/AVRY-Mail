//! POST /v1/ai/draft-reply — write a reply to a conversation for the owner to
//! edit. Nothing is sent or saved: the text goes back to the reply box.
//!
//! The prompt (and its untrusted-content framing) lives in
//! `aivory_mail_core::drafting`. The model is called directly on OpenRouter
//! with the server key, never through the tool-using agent gateway, so text
//! inside an email can't reach anything with side effects.

use std::sync::Arc;

use aivory_mail_core::drafting::{self, TranscriptMessage};
use aivory_mail_storage::db::DbPool;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::api::{authz, AppState};
use crate::mcp_limits::sanitize_for_ai;

const DEFAULT_OPENROUTER_BASE: &str = "https://openrouter.ai/api/v1";
const MODEL_TIMEOUT_SECS: u64 = 45;

fn failure(status: StatusCode, code: &str, message: &str) -> (StatusCode, Json<Value>) {
    (status, Json(serde_json::json!({"success": false, "code": code, "error": message})))
}

type Failure = (StatusCode, Json<Value>);

/// The conversation to answer: its mailbox, the owner's address, and its
/// messages oldest first (drafts, trash and spam left out).
async fn load_conversation(
    state: &Arc<AppState>,
    thread_id: Option<Uuid>,
    message_id: Option<Uuid>,
) -> Result<(String, String, Vec<TranscriptMessage>), Failure> {
    let internal = |_| failure(StatusCode::INTERNAL_SERVER_ERROR, "internal", "internal error");
    let not_found = || failure(StatusCode::NOT_FOUND, "not_found", "conversation not found");
    // (mailbox_id, thread_id or None, single message id)
    let (mailbox_id, thread, single): (String, Option<String>, Option<String>) = match (&state.db, thread_id, message_id) {
        (DbPool::Postgres(pool), Some(t), _) => {
            let r = sqlx::query("SELECT mailbox_id::text AS mailbox_id FROM threads WHERE id=$1")
                .bind(t).fetch_optional(pool).await.map_err(internal)?.ok_or_else(not_found)?;
            (r.get("mailbox_id"), Some(t.to_string()), None)
        }
        (DbPool::Sqlite(pool), Some(t), _) => {
            let r = sqlx::query("SELECT mailbox_id FROM threads WHERE id=?")
                .bind(t.to_string()).fetch_optional(pool).await.map_err(internal)?.ok_or_else(not_found)?;
            (r.get("mailbox_id"), Some(t.to_string()), None)
        }
        (DbPool::Postgres(pool), None, Some(m)) => {
            let r = sqlx::query("SELECT mailbox_id::text AS mailbox_id, thread_id::text AS thread_id FROM messages WHERE id=$1")
                .bind(m).fetch_optional(pool).await.map_err(internal)?.ok_or_else(not_found)?;
            let t: Option<String> = r.get("thread_id");
            (r.get("mailbox_id"), t.clone(), t.is_none().then(|| m.to_string()))
        }
        (DbPool::Sqlite(pool), None, Some(m)) => {
            let r = sqlx::query("SELECT mailbox_id, thread_id FROM messages WHERE id=?")
                .bind(m.to_string()).fetch_optional(pool).await.map_err(internal)?.ok_or_else(not_found)?;
            let t: Option<String> = r.get("thread_id");
            (r.get("mailbox_id"), t.clone(), t.is_none().then(|| m.to_string()))
        }
        (_, None, None) => return Err(failure(StatusCode::BAD_REQUEST, "bad_request", "thread_id or message_id is required")),
    };

    let owner: String = match &state.db {
        DbPool::Postgres(pool) => sqlx::query_scalar::<_, String>("SELECT address FROM mailboxes WHERE id::text=$1")
            .bind(&mailbox_id).fetch_optional(pool).await.map_err(internal)?,
        DbPool::Sqlite(pool) => sqlx::query_scalar::<_, String>("SELECT address FROM mailboxes WHERE id=?")
            .bind(&mailbox_id).fetch_optional(pool).await.map_err(internal)?,
    }
    .ok_or_else(not_found)?
    .to_lowercase();

    let filter = "folder NOT IN ('Drafts','Trash','Spam')";
    let rows: Vec<(String, String, Option<String>, String)> = match (&state.db, &thread, &single) {
        (DbPool::Postgres(pool), Some(t), _) => sqlx::query(&format!("SELECT from_addr, created_at::text AS created_at, body_text, folder FROM messages WHERE thread_id=$1::uuid AND mailbox_id::text=$2 AND {filter} ORDER BY created_at ASC"))
            .bind(t).bind(&mailbox_id).fetch_all(pool).await.map_err(internal)?
            .into_iter().map(|r| (r.get("from_addr"), r.get("created_at"), r.get("body_text"), r.get("folder"))).collect(),
        (DbPool::Sqlite(pool), Some(t), _) => sqlx::query(&format!("SELECT from_addr, created_at, body_text, folder FROM messages WHERE thread_id=? AND mailbox_id=? AND {filter} ORDER BY created_at ASC"))
            .bind(t).bind(&mailbox_id).fetch_all(pool).await.map_err(internal)?
            .into_iter().map(|r| (r.get("from_addr"), r.get("created_at"), r.get("body_text"), r.get("folder"))).collect(),
        (DbPool::Postgres(pool), None, Some(m)) => sqlx::query("SELECT from_addr, created_at::text AS created_at, body_text, folder FROM messages WHERE id=$1::uuid")
            .bind(m).fetch_all(pool).await.map_err(internal)?
            .into_iter().map(|r| (r.get("from_addr"), r.get("created_at"), r.get("body_text"), r.get("folder"))).collect(),
        (DbPool::Sqlite(pool), None, Some(m)) => sqlx::query("SELECT from_addr, created_at, body_text, folder FROM messages WHERE id=?")
            .bind(m).fetch_all(pool).await.map_err(internal)?
            .into_iter().map(|r| (r.get("from_addr"), r.get("created_at"), r.get("body_text"), r.get("folder"))).collect(),
        _ => Vec::new(),
    };
    let messages: Vec<TranscriptMessage> = rows
        .into_iter()
        .map(|(from, date, body, folder)| {
            let from_owner = folder.eq_ignore_ascii_case("Sent") || from.to_lowercase().contains(&owner);
            TranscriptMessage {
                from: sanitize_for_ai(&from),
                date,
                from_owner,
                body: sanitize_for_ai(&body.unwrap_or_default()),
            }
        })
        .collect();
    if messages.is_empty() {
        return Err(not_found());
    }
    Ok((mailbox_id, owner, messages))
}

/// The owner's standing instructions (Settings → Compose → AI drafting).
async fn drafting_instructions(state: &Arc<AppState>, mailbox_id: &str) -> Option<String> {
    match &state.db {
        DbPool::Postgres(pool) => sqlx::query_scalar("SELECT value FROM user_settings WHERE category='ai' AND key='drafting_instructions' AND (mailbox_id=$1 OR mailbox_id IS NULL) ORDER BY (mailbox_id IS NULL) LIMIT 1")
            .bind(mailbox_id).fetch_optional(pool).await.ok().flatten(),
        DbPool::Sqlite(pool) => sqlx::query_scalar("SELECT value FROM user_settings WHERE category='ai' AND key='drafting_instructions' AND (mailbox_id=? OR mailbox_id IS NULL OR mailbox_id='') ORDER BY (mailbox_id IS NULL OR mailbox_id='') LIMIT 1")
            .bind(mailbox_id).fetch_optional(pool).await.ok().flatten(),
    }
}

async fn call_model(state: &Arc<AppState>, messages: Vec<Value>) -> Result<String, Failure> {
    let key = std::env::var("OPENROUTER_API_KEY").ok().filter(|k| !k.trim().is_empty()).ok_or_else(|| {
        failure(StatusCode::SERVICE_UNAVAILABLE, "ai_not_configured", "AI drafting isn't set up on this server.")
    })?;
    let base = std::env::var("OPENROUTER_BASE_URL").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| DEFAULT_OPENROUTER_BASE.into());
    let unavailable = || failure(StatusCode::BAD_GATEWAY, "ai_unavailable", "The AI model didn't answer. Try again in a moment.");
    let resp = reqwest::Client::new()
        .post(format!("{}/chat/completions", base.trim_end_matches('/')))
        .bearer_auth(key)
        .header("HTTP-Referer", "https://mail.aivory.uk")
        .header("X-Title", "Aivory Mail Draft Reply")
        .json(&serde_json::json!({
            "model": state.config.mail_intelligence_model,
            "messages": messages,
            "temperature": 0.4,
            "max_tokens": 900,
        }))
        .timeout(std::time::Duration::from_secs(MODEL_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| { tracing::warn!("draft-reply model request failed: {}", e); unavailable() })?;
    if !resp.status().is_success() {
        tracing::warn!("draft-reply model returned {}", resp.status());
        return Err(unavailable());
    }
    let body: Value = resp.json().await.map_err(|_| unavailable())?;
    let text = body["choices"][0]["message"]["content"].as_str().unwrap_or("");
    let draft = drafting::clean_output(text);
    if draft.is_empty() {
        return Err(unavailable());
    }
    Ok(draft)
}

pub async fn draft_reply(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let parse = |k: &str| -> Result<Option<Uuid>, StatusCode> {
        body.get(k)
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty())
            .map(|v| Uuid::parse_str(v.trim()).map_err(|_| StatusCode::BAD_REQUEST))
            .transpose()
    };
    let thread_id = parse("thread_id")?;
    let message_id = parse("message_id")?;
    // Same ownership gates as reading the conversation itself.
    match (thread_id, message_id) {
        (Some(t), _) => authz::require_thread_access(&state, &headers, t).await?,
        (None, Some(m)) => authz::require_message_access(&state, &headers, m).await?,
        (None, None) => return Err(StatusCode::BAD_REQUEST),
    }
    let note = body.get("hint").and_then(Value::as_str);

    let (mailbox_id, owner, messages) = match load_conversation(&state, thread_id, message_id).await {
        Ok(v) => v,
        Err(f) => return Ok(f),
    };
    let instructions = drafting_instructions(&state, &mailbox_id).await;
    let prompt = drafting::build_messages(&owner, &messages, instructions.as_deref(), note);
    match call_model(&state, prompt).await {
        Ok(draft) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "data": {"draft": draft, "model": state.config.mail_intelligence_model}})),
        )),
        Err(f) => Ok(f),
    }
}
