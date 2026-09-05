use std::sync::Arc;
use axum::{extract::{State, Path, Query}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

pub async fn list(State(state): State<Arc<AppState>>, Query(params): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = params.get("mailbox_id").and_then(|v| v.as_str());
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            // message_count/last_message_at/has_unread on `threads` are only
            // ever written at thread-creation time (find_or_create_thread) —
            // a later message joining the thread, or a read/unread toggle on
            // any of its messages, never updates them. That's why "mark all
            // as read" looked right until the next refresh re-read these
            // frozen columns. Compute all three live from `messages` instead
            // of trusting the denormalized cache.
            let r = if let Some(mid) = mailbox_id {
                let uid = Uuid::parse_str(mid).map_err(|_| StatusCode::BAD_REQUEST)?;
                sqlx::query(r#"SELECT t.id, t.subject, t.participant_addrs,
                    (SELECT COUNT(*) FROM messages m WHERE m.thread_id=t.id)::int AS message_count,
                    COALESCE((SELECT MAX(m.created_at) FROM messages m WHERE m.thread_id=t.id), t.last_message_at) AS last_message_at,
                    EXISTS(SELECT 1 FROM messages m WHERE m.thread_id=t.id AND m.is_read=false) AS has_unread
                    FROM threads t WHERE t.mailbox_id=$1 ORDER BY last_message_at DESC LIMIT 50"#)
                    .bind(uid).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            } else {
                sqlx::query(r#"SELECT t.id, t.subject, t.participant_addrs,
                    (SELECT COUNT(*) FROM messages m WHERE m.thread_id=t.id)::int AS message_count,
                    COALESCE((SELECT MAX(m.created_at) FROM messages m WHERE m.thread_id=t.id), t.last_message_at) AS last_message_at,
                    EXISTS(SELECT 1 FROM messages m WHERE m.thread_id=t.id AND m.is_read=false) AS has_unread
                    FROM threads t ORDER BY last_message_at DESC LIMIT 50"#)
                    .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            };
            r.into_iter().map(|row| serde_json::json!({
                "id": row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("id").unwrap_or_default()),
                "subject": row.get::<Option<String>,_>("subject"),
                "participants": row.get::<String,_>("participant_addrs"),
                "message_count": row.get::<i32,_>("message_count"),
                "has_unread": row.try_get::<bool,_>("has_unread").unwrap_or_else(|_| row.try_get::<i32,_>("has_unread").map(|i| i!=0).unwrap_or(false)),
                "last_message_at": row.try_get::<chrono::DateTime<chrono::Utc>,_>("last_message_at").unwrap_or_else(|_| chrono::DateTime::parse_from_rfc3339(&row.try_get::<String,_>("last_message_at").unwrap_or_default()).map(|d| d.with_timezone(&chrono::Utc)).unwrap_or(chrono::Utc::now())).to_rfc3339(),
            })).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = if let Some(mid) = mailbox_id {
                sqlx::query(r#"SELECT t.id, t.subject, t.participant_addrs,
                    (SELECT COUNT(*) FROM messages m WHERE m.thread_id=t.id) AS message_count,
                    COALESCE((SELECT MAX(m.created_at) FROM messages m WHERE m.thread_id=t.id), t.last_message_at) AS last_message_at,
                    EXISTS(SELECT 1 FROM messages m WHERE m.thread_id=t.id AND m.is_read=0) AS has_unread
                    FROM threads t WHERE t.mailbox_id=? ORDER BY last_message_at DESC LIMIT 50"#)
                    .bind(mid).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            } else {
                sqlx::query(r#"SELECT t.id, t.subject, t.participant_addrs,
                    (SELECT COUNT(*) FROM messages m WHERE m.thread_id=t.id) AS message_count,
                    COALESCE((SELECT MAX(m.created_at) FROM messages m WHERE m.thread_id=t.id), t.last_message_at) AS last_message_at,
                    EXISTS(SELECT 1 FROM messages m WHERE m.thread_id=t.id AND m.is_read=0) AS has_unread
                    FROM threads t ORDER BY last_message_at DESC LIMIT 50"#)
                    .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            };
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<String,_>("id"),
                "subject": row.get::<Option<String>,_>("subject"),
                "message_count": row.get::<i32,_>("message_count"),
                "has_unread": row.get::<i32,_>("has_unread") != 0,
                "last_message_at": row.get::<String,_>("last_message_at"),
            })).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn get_one(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let thread: Value = match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT id, subject, participant_addrs FROM threads WHERE id=$1")
                .bind(uid).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
            let msgs = sqlx::query("SELECT id, from_addr, to_addrs, subject, snippet, body_text, body_html, folder, headers_json, created_at FROM messages WHERE thread_id=$1 ORDER BY created_at ASC")
                .bind(uid).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let messages: Vec<Value> = msgs.into_iter().map(|r| serde_json::json!({
                "id": r.get::<Uuid,_>("id").to_string(),
                "from": r.get::<String,_>("from_addr"),
                "to": r.get::<String,_>("to_addrs"),
                "subject": r.get::<Option<String>,_>("subject"),
                "snippet": r.get::<Option<String>,_>("snippet"),
                "body_text": r.get::<Option<String>,_>("body_text"),
                "body_html": r.get::<Option<String>,_>("body_html"),
                "folder": r.get::<String,_>("folder"),
                "headers": r.get::<Option<serde_json::Value>,_>("headers_json"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
            })).collect();
            serde_json::json!({"id": row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("id").unwrap_or_default()), "subject": row.get::<Option<String>,_>("subject"), "participants": row.get::<String,_>("participant_addrs"), "messages": messages})
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id, subject, participant_addrs FROM threads WHERE id=?")
                .bind(uid.to_string()).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
            let msgs = sqlx::query("SELECT id, from_addr, to_addrs, subject, snippet, body_text, body_html, folder, headers_json, created_at FROM messages WHERE thread_id=? ORDER BY created_at ASC")
                .bind(uid.to_string()).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let messages: Vec<Value> = msgs.into_iter().map(|r| serde_json::json!({
                "id": r.get::<String,_>("id"),
                "from": r.get::<String,_>("from_addr"),
                "to": r.get::<String,_>("to_addrs"),
                "subject": r.get::<Option<String>,_>("subject"),
                "snippet": r.get::<Option<String>,_>("snippet"),
                "body_text": r.get::<Option<String>,_>("body_text"),
                "body_html": r.get::<Option<String>,_>("body_html"),
                "folder": r.get::<String,_>("folder"),
                "headers": r.get::<Option<String>,_>("headers_json").and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()),
                "created_at": r.get::<String,_>("created_at"),
            })).collect();
            serde_json::json!({"id": row.get::<String,_>("id"), "subject": row.get::<Option<String>,_>("subject"), "participants": row.get::<String,_>("participant_addrs"), "messages": messages})
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": thread})))
}

pub async fn reply(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let thread_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let (subject, mailbox_addr): (String, String) = match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT t.subject, m.address FROM threads t JOIN mailboxes m ON m.id=t.mailbox_id WHERE t.id=$1")
                .bind(thread_id).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
            (row.get::<Option<String>,_>("subject").unwrap_or_default(), row.get::<String,_>("address"))
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT t.subject, m.address FROM threads t JOIN mailboxes m ON m.id=t.mailbox_id WHERE t.id=?")
                .bind(thread_id.to_string()).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
            (row.get::<Option<String>,_>("subject").unwrap_or_default(), row.get::<String,_>("address"))
        }
    };
    let text = body.get("text").and_then(|v| v.as_str()).unwrap_or("");
    let html = body.get("html").and_then(|v| v.as_str()).map(|s| s.to_string());
    let to = body.get("to").and_then(|v| v.as_array()).map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<_>>()).unwrap_or_default();
    if to.is_empty() { return Err(StatusCode::BAD_REQUEST); }

    let req = aivory_mail_core::types::SendRequest {
        from: mailbox_addr,
        to,
        cc: None, bcc: None,
        subject: if subject.to_lowercase().starts_with("re:") { subject } else { format!("Re: {}", subject) },
        text: Some(text.to_string()),
        html,
        attachments: None,
        thread_id: Some(thread_id),
        in_reply_to: None,
    };
    // Detached so a client that closes the tab mid-send can't cancel a send
    // that's already left the building — see the matching comment in send.rs.
    let state2 = state.clone();
    let handle = tokio::spawn(async move { crate::mail::outbound::send_email(&state2, req).await });
    let mid = handle.await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"success": true, "data": {"id": mid.to_string()}})))
}

// Crawl + follow-up analysis
pub async fn crawl(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let thread = get_one(State(state.clone()), Path(id.clone())).await?;
    let data = thread.0.get("data").cloned().unwrap_or(serde_json::Value::Null);
    let messages = data.get("messages").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let count = messages.len() as i32;
    let last = messages.last().and_then(|m| m.get("created_at").and_then(|v| v.as_str())).unwrap_or("");
    let last_folder = messages.last().and_then(|m| m.get("folder").and_then(|v| v.as_str())).unwrap_or("Inbox");
    // follow-up heuristic: if last is Sent and >2 days old and count <6, needs follow-up
    let days_since = if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(last) { (chrono::Utc::now() - dt.with_timezone(&chrono::Utc)).num_days() } else { 0 };
    let needs_follow_up = last_folder == "Sent" && days_since >= 2 && count < 10;
    let suggested = if needs_follow_up {
        let subj = data.get("subject").and_then(|v| v.as_str()).unwrap_or("");
        let subj2 = if subj.to_lowercase().starts_with("re:") { subj.to_string() } else { format!("Re: {}", subj) };
        let body = format!("Hi,\n\nJust following up on \"{}\" — wanted to check if you had a chance to review.\n\nLet me know a good time to connect, or book: https://book.aivory.uk/book/aivory-call\n\nBest,\nAivory Team", subj);
        let reason = format!("Last sent {} days ago, no reply in thread ({} messages)", days_since, count);
        serde_json::json!({"subject": subj2, "body": body, "reason": reason})
    } else { serde_json::Value::Null };

    // participants timeline
    let timeline: Vec<Value> = messages.iter().enumerate().map(|(i,m)| serde_json::json!({
        "idx": i+1,
        "from": m.get("from"),
        "snippet": m.get("snippet"),
        "at": m.get("created_at"),
        "is_outbound": m.get("folder").and_then(|v| v.as_str()) == Some("Sent")
    })).collect();

    Ok(Json(serde_json::json!({"success": true, "data": {
        "thread": data,
        "crawl": {"message_count": count, "days_since_last": days_since, "last_folder": last_folder, "needs_follow_up": needs_follow_up, "suggested_follow_up": suggested, "timeline": timeline}
    }})))
}

pub async fn follow_up(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    // generate follow-up draft via crawl suggestion
    let crawled = crawl(State(state.clone()), Path(id.clone())).await?;
    let suggested = crawled.0.get("data").and_then(|d| d.get("crawl")).and_then(|c| c.get("suggested_follow_up")).cloned().unwrap_or(serde_json::Value::Null);
    if suggested.is_null() {
        return Ok(Json(serde_json::json!({"success": true, "data": {"needed": false, "reason": "No follow-up needed at this time"}})));
    }
    Ok(Json(serde_json::json!({"success": true, "data": {"needed": true, "draft": suggested}})))
}
