use crate::api::{authz, AppState};
use aivory_mail_storage::db::DbPool;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::Value;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

const VALID_STATES: &[&str] = &[
    "needs_reply",
    "waiting_on_me",
    "waiting_on_them",
    "fyi",
    "auto_handled",
    "needs_approval",
    "done",
];

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<Value>,
) -> Result<Json<Value>, StatusCode> {
    let state_filter = q.get("state").and_then(|v| v.as_str());
    if let Some(s) = state_filter {
        if !VALID_STATES.contains(&s) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    let requested = q.get("mailbox_id").and_then(|v| v.as_str());
    let mailbox_id = authz::mailbox_scope(&state, &headers, requested)
        .await?
        .map(|id| id.to_string());
    let limit: i64 = crate::api::query_i64(q.get("limit"))
        .unwrap_or(50)
        .clamp(1, 100);
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let mut sql = String::from("SELECT id, mailbox_id, thread_id, message_id, type, state, title, body, payload, created_at, updated_at FROM agent_tasks WHERE tenant_id::text='default'");
            if mailbox_id.is_some() {
                sql.push_str(" AND mailbox_id=$1");
            }
            if state_filter.is_some() {
                sql.push_str(if mailbox_id.is_some() {
                    " AND state=$2"
                } else {
                    " AND state=$1"
                });
            }
            let limit_slot = if mailbox_id.is_some() {
                if state_filter.is_some() {
                    3
                } else {
                    2
                }
            } else {
                if state_filter.is_some() {
                    2
                } else {
                    1
                }
            };
            sql.push_str(&format!(" ORDER BY updated_at DESC LIMIT ${}", limit_slot));
            let mut query = sqlx::query(&sql);
            if let Some(mb) = mailbox_id.as_deref() {
                query = query.bind(mb);
            }
            if let Some(st) = state_filter {
                query = query.bind(st);
            }
            let r = query
                .bind(limit)
                .fetch_all(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| {
                let id_str = row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("id").unwrap_or_default());
                serde_json::json!({
                    "id": id_str,
                    "mailbox_id": row.get::<Option<String>,_>("mailbox_id"),
                    "thread_id": row.get::<Option<String>,_>("thread_id"),
                    "message_id": row.get::<Option<String>,_>("message_id"),
                    "type": row.get::<String,_>("type"),
                    "state": row.get::<String,_>("state"),
                    "title": row.get::<String,_>("title"),
                    "body": row.get::<String,_>("body"),
                    "payload": row.try_get::<Value,_>("payload").unwrap_or_else(|_| serde_json::from_str(&row.try_get::<String,_>("payload").unwrap_or("{}".into())).unwrap_or(Value::Null)),
                    "created_at": row.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at").map(|d| d.to_rfc3339()).unwrap_or_else(|_| row.try_get::<String,_>("created_at").unwrap_or_default()),
                    "updated_at": row.try_get::<chrono::DateTime<chrono::Utc>,_>("updated_at").map(|d| d.to_rfc3339()).unwrap_or_else(|_| row.try_get::<String,_>("updated_at").unwrap_or_default())
                })
            }).collect()
        }
        DbPool::Sqlite(pool) => {
            let mut sql = String::from("SELECT id, mailbox_id, thread_id, message_id, type, state, title, body, payload, created_at, updated_at FROM agent_tasks WHERE tenant_id=?");
            if mailbox_id.is_some() {
                sql.push_str(" AND mailbox_id=?");
            }
            if state_filter.is_some() {
                sql.push_str(" AND state=?");
            }
            sql.push_str(" ORDER BY updated_at DESC LIMIT ?");
            let mut query = sqlx::query(&sql).bind("default");
            if let Some(mb) = mailbox_id.as_deref() {
                query = query.bind(mb);
            }
            if let Some(st) = state_filter {
                query = query.bind(st);
            }
            let r = query
                .bind(limit)
                .fetch_all(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<String,_>("id"),
                "mailbox_id": row.get::<Option<String>,_>("mailbox_id"),
                "thread_id": row.get::<Option<String>,_>("thread_id"),
                "message_id": row.get::<Option<String>,_>("message_id"),
                "type": row.get::<String,_>("type"),
                "state": row.get::<String,_>("state"),
                "title": row.get::<String,_>("title"),
                "body": row.get::<String,_>("body"),
                "payload": serde_json::from_str::<Value>(&row.get::<String,_>("payload")).unwrap_or(Value::Null),
                "created_at": row.get::<String,_>("created_at"),
                "updated_at": row.get::<String,_>("updated_at")
            })).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let typ = body
        .get("type")
        .or(body.get("action"))
        .and_then(|v| v.as_str())
        .unwrap_or("triage")
        .to_string();
    let state_val = body
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("needs_reply")
        .to_string();
    if !VALID_STATES.contains(&state_val.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let requested = body.get("mailbox_id").and_then(|v| v.as_str());
    let mailbox_id = authz::mailbox_scope(&state, &headers, requested)
        .await?
        .map(|id| id.to_string());
    let thread_id = body
        .get("thread_id")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let message_id = body
        .get("message_id")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if let Some(mid) = message_id.as_deref() {
        authz::require_message_access(
            &state,
            &headers,
            Uuid::parse_str(mid).map_err(|_| StatusCode::BAD_REQUEST)?,
        )
        .await?;
    }
    if let Some(tid) = thread_id.as_deref() {
        authz::require_thread_access(
            &state,
            &headers,
            Uuid::parse_str(tid).map_err(|_| StatusCode::BAD_REQUEST)?,
        )
        .await?;
    }
    let title = body
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Agent task")
        .to_string();
    let bdy = body
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let payload = body
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::json!({}));
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
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"success": true, "data": {"id": id.to_string()}})),
    ))
}

async fn task_mailbox_id(state: &Arc<AppState>, id: Uuid) -> Result<String, StatusCode> {
    match &state.db {
        DbPool::Postgres(pool) => sqlx::query("SELECT mailbox_id FROM agent_tasks WHERE id=$1")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .and_then(|r| r.get::<Option<String>, _>("mailbox_id"))
            .ok_or(StatusCode::NOT_FOUND),
        DbPool::Sqlite(pool) => sqlx::query("SELECT mailbox_id FROM agent_tasks WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .and_then(|r| r.get::<Option<String>, _>("mailbox_id"))
            .ok_or(StatusCode::NOT_FOUND),
    }
}

pub async fn get_one(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let mailbox_id = task_mailbox_id(&state, uid).await?;
    authz::mailbox_scope(&state, &headers, Some(&mailbox_id)).await?;
    let row: Option<Value> = match &state.db {
        DbPool::Postgres(pool) => sqlx::query("SELECT id, mailbox_id, state, title, body, payload FROM agent_tasks WHERE id=$1").bind(uid).fetch_optional(pool).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.map(|row| serde_json::json!({"id": row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("id").unwrap_or_default()), "mailbox_id": row.get::<Option<String>,_>("mailbox_id"), "state": row.get::<String,_>("state"), "title": row.get::<String,_>("title"), "body": row.get::<String,_>("body"), "payload": row.try_get::<Value,_>("payload").unwrap_or_else(|_| serde_json::from_str(&row.try_get::<String,_>("payload").unwrap_or("{}".into())).unwrap_or(Value::Null))})),
        DbPool::Sqlite(pool) => sqlx::query("SELECT id, mailbox_id, state, title, body, payload FROM agent_tasks WHERE id=?").bind(uid.to_string()).fetch_optional(pool).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.map(|row| serde_json::json!({"id": row.get::<String,_>("id"), "mailbox_id": row.get::<Option<String>,_>("mailbox_id"), "state": row.get::<String,_>("state"), "title": row.get::<String,_>("title"), "body": row.get::<String,_>("body"), "payload": serde_json::from_str::<Value>(&row.get::<String,_>("payload")).unwrap_or(Value::Null)})),
    };
    row.map(|v| Json(serde_json::json!({"success": true, "data": v})))
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let mailbox_id = task_mailbox_id(&state, uid).await?;
    authz::mailbox_scope(&state, &headers, Some(&mailbox_id)).await?;
    let new_state = body.get("state").and_then(|v| v.as_str());
    if let Some(s) = new_state {
        if !VALID_STATES.contains(&s) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    let title = body.get("title").and_then(|v| v.as_str());
    match &state.db {
        DbPool::Postgres(pool) => {
            if let Some(s) = new_state {
                sqlx::query("UPDATE agent_tasks SET state=$1, updated_at=NOW() WHERE id=$2 AND mailbox_id=$3").bind(s).bind(uid).bind(&mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            if let Some(t) = title {
                sqlx::query("UPDATE agent_tasks SET title=$1, updated_at=NOW() WHERE id=$2 AND mailbox_id=$3").bind(t).bind(uid).bind(&mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
        }
        DbPool::Sqlite(pool) => {
            if let Some(s) = new_state {
                sqlx::query(
                    "UPDATE agent_tasks SET state=?, updated_at=? WHERE id=? AND mailbox_id=?",
                )
                .bind(s)
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(uid.to_string())
                .bind(&mailbox_id)
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            if let Some(t) = title {
                sqlx::query(
                    "UPDATE agent_tasks SET title=?, updated_at=? WHERE id=? AND mailbox_id=?",
                )
                .bind(t)
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(uid.to_string())
                .bind(&mailbox_id)
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

/// Verify that internal task references are all rooted in the assigned mailbox.
/// The inbound caller derives these IDs from one stored message, but this guard
/// keeps the non-HTTP boundary safe if another internal caller is added later.
async fn validate_internal_relationships(
    state: &Arc<AppState>,
    mailbox_id: &str,
    thread_id: Option<&str>,
    message_id: Option<&str>,
) -> Result<(), StatusCode> {
    let mailbox_uuid = Uuid::parse_str(mailbox_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let thread_uuid = thread_id
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let message_uuid = message_id
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    match &state.db {
        DbPool::Postgres(pool) => {
            let mailbox_exists = sqlx::query("SELECT 1 FROM mailboxes WHERE id=$1")
                .bind(mailbox_uuid)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .is_some();
            if !mailbox_exists {
                return Err(StatusCode::NOT_FOUND);
            }

            if let Some(thread_uuid) = thread_uuid {
                let thread_exists =
                    sqlx::query("SELECT 1 FROM threads WHERE id=$1 AND mailbox_id=$2")
                        .bind(thread_uuid)
                        .bind(mailbox_uuid)
                        .fetch_optional(pool)
                        .await
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                        .is_some();
                if !thread_exists {
                    return Err(StatusCode::NOT_FOUND);
                }
            }

            if let Some(message_uuid) = message_uuid {
                let message_thread =
                    sqlx::query("SELECT thread_id FROM messages WHERE id=$1 AND mailbox_id=$2")
                        .bind(message_uuid)
                        .bind(mailbox_uuid)
                        .fetch_optional(pool)
                        .await
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                        .map(|row| row.get::<Option<Uuid>, _>("thread_id"));
                let Some(message_thread) = message_thread else {
                    return Err(StatusCode::NOT_FOUND);
                };
                if thread_uuid.is_some() && message_thread != thread_uuid {
                    return Err(StatusCode::NOT_FOUND);
                }
            }
        }
        DbPool::Sqlite(pool) => {
            let mailbox_exists = sqlx::query("SELECT 1 FROM mailboxes WHERE id=?")
                .bind(mailbox_id)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .is_some();
            if !mailbox_exists {
                return Err(StatusCode::NOT_FOUND);
            }

            if let Some(thread_uuid) = thread_uuid {
                let thread_exists =
                    sqlx::query("SELECT 1 FROM threads WHERE id=? AND mailbox_id=?")
                        .bind(thread_uuid.to_string())
                        .bind(mailbox_id)
                        .fetch_optional(pool)
                        .await
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                        .is_some();
                if !thread_exists {
                    return Err(StatusCode::NOT_FOUND);
                }
            }

            if let Some(message_uuid) = message_uuid {
                let message_thread =
                    sqlx::query("SELECT thread_id FROM messages WHERE id=? AND mailbox_id=?")
                        .bind(message_uuid.to_string())
                        .bind(mailbox_id)
                        .fetch_optional(pool)
                        .await
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                        .map(|row| row.get::<Option<String>, _>("thread_id"));
                let Some(message_thread) = message_thread else {
                    return Err(StatusCode::NOT_FOUND);
                };
                if let Some(thread_uuid) = thread_uuid {
                    if message_thread.as_deref() != Some(thread_uuid.to_string().as_str()) {
                        return Err(StatusCode::NOT_FOUND);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Internal ingestion path used by the inbound pipeline after it has already
/// authenticated and assigned the mailbox. It is not routed over HTTP.
pub async fn create_internal(state: Arc<AppState>, body: Value) -> Result<(), StatusCode> {
    let mailbox_id = body
        .get("mailbox_id")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_string();
    Uuid::parse_str(&mailbox_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let typ = body
        .get("type")
        .or(body.get("action"))
        .and_then(|v| v.as_str())
        .unwrap_or("triage");
    let state_val = body
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("needs_reply");
    if !VALID_STATES.contains(&state_val) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let title = body
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Agent task");
    let bdy = body.get("body").and_then(|v| v.as_str()).unwrap_or("");
    let thread_id = body.get("thread_id").and_then(|v| v.as_str());
    let message_id = body.get("message_id").and_then(|v| v.as_str());
    validate_internal_relationships(&state, &mailbox_id, thread_id, message_id).await?;
    let payload = body
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let id = Uuid::new_v4();
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO agent_tasks (id, tenant_id, mailbox_id, thread_id, message_id, type, state, title, body, payload, created_at, updated_at) VALUES ($1,'default',$2,$3,$4,$5,$6,$7,$8,$9,NOW(),NOW())")
                .bind(id).bind(&mailbox_id).bind(thread_id).bind(message_id).bind(typ).bind(state_val).bind(title).bind(bdy).bind(&payload)
                .execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO agent_tasks (id, tenant_id, mailbox_id, thread_id, message_id, type, state, title, body, payload, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id.to_string()).bind("default").bind(&mailbox_id).bind(thread_id).bind(message_id).bind(typ).bind(state_val).bind(title).bind(bdy).bind(serde_json::to_string(&payload).unwrap()).bind(chrono::Utc::now().to_rfc3339()).bind(chrono::Utc::now().to_rfc3339())
                .execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(())
}
