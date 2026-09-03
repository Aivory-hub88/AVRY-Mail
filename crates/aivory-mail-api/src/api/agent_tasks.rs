use std::sync::Arc;
use axum::{extract::{State, Path, Query}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

const VALID_STATES: &[&str] = &["needs_reply","waiting_on_me","waiting_on_them","fyi","auto_handled","needs_approval","done"];

pub async fn list(State(state): State<Arc<AppState>>, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let state_filter = q.get("state").and_then(|v| v.as_str());
    let mailbox_id = q.get("mailbox_id").and_then(|v| v.as_str());
    let limit: i64 = q.get("limit").and_then(|v| v.as_i64()).unwrap_or(50).min(100);
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let mut sql = String::from("SELECT id, mailbox_id, thread_id, message_id, type, state, title, body, payload, created_at, updated_at FROM agent_tasks WHERE tenant_id='default'");
            if state_filter.is_some() { sql.push_str(" AND state=$2"); }
            if mailbox_id.is_some() { sql.push_str(" AND mailbox_id=$3"); }
            sql.push_str(" ORDER BY updated_at DESC LIMIT $4");
            // Simplified: build query dynamically with bind order handling is complex; for now just filter in Rust if needed
            let r = sqlx::query("SELECT id, mailbox_id, thread_id, message_id, type, state, title, body, payload, created_at, updated_at FROM agent_tasks WHERE tenant_id='default' ORDER BY updated_at DESC LIMIT $1")
                .bind(limit).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut out = Vec::new();
            for row in r {
                let st: String = row.get("state");
                if let Some(f) = state_filter { if st != f { continue; } }
                if let Some(mb) = mailbox_id { let mid: Option<String> = row.get("mailbox_id"); if mid.as_deref()!=Some(mb) { continue; } }
                out.push(serde_json::json!({
                    "id": row.get::<Uuid,_>("id").to_string(),
                    "mailbox_id": row.get::<Option<String>,_>("mailbox_id"),
                    "thread_id": row.get::<Option<String>,_>("thread_id"),
                    "message_id": row.get::<Option<String>,_>("message_id"),
                    "type": row.get::<String,_>("type"),
                    "state": st,
                    "title": row.get::<String,_>("title"),
                    "body": row.get::<String,_>("body"),
                    "payload": row.get::<Value,_>("payload"),
                    "created_at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
                    "updated_at": row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at").to_rfc3339()
                }));
                if out.len() as i64 >= limit { break; }
            }
            out
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, mailbox_id, thread_id, message_id, type, state, title, body, payload, created_at, updated_at FROM agent_tasks WHERE tenant_id='default' ORDER BY updated_at DESC LIMIT ?")
                .bind(limit).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut out = Vec::new();
            for row in r {
                let st: String = row.get("state");
                if let Some(f) = state_filter { if st != f { continue; } }
                if let Some(mb) = mailbox_id { let mid: Option<String> = row.get("mailbox_id"); if mid.as_deref()!=Some(mb) { continue; } }
                out.push(serde_json::json!({
                    "id": row.get::<String,_>("id"),
                    "mailbox_id": row.get::<Option<String>,_>("mailbox_id"),
                    "thread_id": row.get::<Option<String>,_>("thread_id"),
                    "message_id": row.get::<Option<String>,_>("message_id"),
                    "type": row.get::<String,_>("type"),
                    "state": st,
                    "title": row.get::<String,_>("title"),
                    "body": row.get::<String,_>("body"),
                    "payload": serde_json::from_str::<Value>(&row.get::<String,_>("payload")).unwrap_or(Value::Null),
                    "created_at": row.get::<String,_>("created_at"),
                    "updated_at": row.get::<String,_>("updated_at")
                }));
                if out.len() as i64 >= limit { break; }
            }
            out
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn create(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let typ = body.get("type").or(body.get("action")).and_then(|v| v.as_str()).unwrap_or("triage").to_string();
    let state_val = body.get("state").and_then(|v| v.as_str()).unwrap_or("needs_reply").to_string();
    if !VALID_STATES.contains(&state_val.as_str()) { return Err(StatusCode::BAD_REQUEST); }
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("Agent task").to_string();
    let bdy = body.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mailbox_id = body.get("mailbox_id").and_then(|v| v.as_str()).map(|s| s.to_string());
    let thread_id = body.get("thread_id").and_then(|v| v.as_str()).map(|s| s.to_string());
    let message_id = body.get("message_id").and_then(|v| v.as_str()).map(|s| s.to_string());
    let payload = body.get("payload").cloned().unwrap_or(serde_json::json!({}));
    let id = Uuid::new_v4();
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO agent_tasks (id, tenant_id, mailbox_id, thread_id, message_id, type, state, title, body, payload, created_at, updated_at) VALUES ($1,'default',$2,$3,$4,$5,$6,$7,$8,$9,NOW(),NOW())")
                .bind(id).bind(&mailbox_id).bind(&thread_id).bind(&message_id).bind(&typ).bind(&state_val).bind(&title).bind(&bdy).bind(&payload)
                .execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO agent_tasks (id, tenant_id, mailbox_id, thread_id, message_id, type, state, title, body, payload, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id.to_string()).bind("default").bind(&mailbox_id).bind(&thread_id).bind(&message_id).bind(&typ).bind(&state_val).bind(&title).bind(&bdy).bind(serde_json::to_string(&payload).unwrap()).bind(chrono::Utc::now().to_rfc3339()).bind(chrono::Utc::now().to_rfc3339())
                .execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({"success": true, "data": {"id": id.to_string()}}))))
}

pub async fn get_one(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let row: Option<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, mailbox_id, state, title, body, payload FROM agent_tasks WHERE id=$1").bind(uid).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.map(|row| serde_json::json!({
                "id": row.get::<Uuid,_>("id").to_string(),
                "mailbox_id": row.get::<Option<String>,_>("mailbox_id"),
                "state": row.get::<String,_>("state"),
                "title": row.get::<String,_>("title"),
                "body": row.get::<String,_>("body"),
                "payload": row.get::<Value,_>("payload")
            }))
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, mailbox_id, state, title, body, payload FROM agent_tasks WHERE id=?").bind(uid.to_string()).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.map(|row| serde_json::json!({
                "id": row.get::<String,_>("id"),
                "mailbox_id": row.get::<Option<String>,_>("mailbox_id"),
                "state": row.get::<String,_>("state"),
                "title": row.get::<String,_>("title"),
                "body": row.get::<String,_>("body"),
                "payload": serde_json::from_str::<Value>(&row.get::<String,_>("payload")).unwrap_or(Value::Null)
            }))
        }
    };
    if let Some(v) = row { Ok(Json(serde_json::json!({"success": true, "data": v}))) } else { Err(StatusCode::NOT_FOUND) }
}

pub async fn update(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let new_state = body.get("state").and_then(|v| v.as_str());
    if let Some(s) = new_state { if !VALID_STATES.contains(&s) { return Err(StatusCode::BAD_REQUEST); } }
    let title = body.get("title").and_then(|v| v.as_str());
    match &state.db {
        DbPool::Postgres(pool) => {
            if let Some(s) = new_state { sqlx::query("UPDATE agent_tasks SET state=$1, updated_at=NOW() WHERE id=$2").bind(s).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(t) = title { sqlx::query("UPDATE agent_tasks SET title=$1, updated_at=NOW() WHERE id=$2").bind(t).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        }
        DbPool::Sqlite(pool) => {
            if let Some(s) = new_state { sqlx::query("UPDATE agent_tasks SET state=?, updated_at=? WHERE id=?").bind(s).bind(chrono::Utc::now().to_rfc3339()).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(t) = title { sqlx::query("UPDATE agent_tasks SET title=?, updated_at=? WHERE id=?").bind(t).bind(chrono::Utc::now().to_rfc3339()).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}
