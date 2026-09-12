use crate::api::{authz, AppState};
use super::execution_context::ExecutionContext;
use aivory_mail_storage::db::DbPool;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::Value;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::RwLock;
use uuid::Uuid;

static CACHE: OnceLock<RwLock<HashMap<String, (Value, chrono::DateTime<chrono::Utc>)>>> =
    OnceLock::new();
fn cache() -> &'static RwLock<HashMap<String, ValueWithTime>> {
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}
type ValueWithTime = (Value, chrono::DateTime<chrono::Utc>);

pub async fn compile(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<Value>,
) -> Result<Json<Value>, StatusCode> {
    let budget: usize = q.get("budget").and_then(|v| v.as_u64()).unwrap_or(4000) as usize;
    let requested = q.get("mailbox_id").and_then(|v| v.as_str());
    let mailbox_id = authz::mailbox_scope(&state, &headers, requested)
        .await?
        .ok_or(StatusCode::FORBIDDEN)?;
    let mailbox_id_text = mailbox_id.to_string();
    let tenant = format!("mailbox:{}", mailbox_id_text);
    let cache_key = format!("{}:{}", mailbox_id_text, budget);
    // Try cache 30s
    {
        let c = cache().read().await;
        if let Some((v, t)) = c.get(&cache_key) {
            if (chrono::Utc::now() - *t).num_seconds() < 30 {
                return Ok(Json(
                    serde_json::json!({"success": true, "cached": true, "data": v}),
                ));
            }
        }
    }

    // Compile all scopes in parallel-ish (sequential for sqlite)
    let inbox = compile_folder(&state, mailbox_id, "Inbox", 3, 300).await;
    let sent = compile_folder(&state, mailbox_id, "Sent", 3, 200).await;
    let drafts = compile_folder(&state, mailbox_id, "Drafts", 3, 200).await;
    let trash = compile_folder(&state, mailbox_id, "Trash", 2, 150).await;
    let spam = compile_folder(&state, mailbox_id, "Spam", 2, 150).await;
    let calendar = compile_calendar(&state, &mailbox_id_text, 5).await;
    let overview = compile_overview(&state, mailbox_id).await;
    let threads_crawl = compile_threads_needing(&state, mailbox_id).await;

    let mut compiled = serde_json::json!({
        "tenant": tenant,
        "budget": budget,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "overview": overview,
        "inbox": inbox,
        "sent": sent,
        "drafts": drafts,
        "trash": trash,
        "spam": spam,
        "calendar": calendar,
        "threads_needing_follow_up": threads_crawl,
        "hint": "auto-compiled knowledge list — 1 call vs scan per folder; feed to deepseek/Cognee, not raw list"
    });

    // Trim to budget (approx chars)
    let total_chars = compiled.to_string().len();
    if total_chars > budget * 4 {
        // naive trim: shorten inbox messages
        if let Some(arr) = compiled
            .get_mut("inbox")
            .and_then(|v| v.get_mut("top"))
            .and_then(|v| v.as_array_mut())
        {
            arr.truncate(2);
        }
    }

    // Cache
    {
        let mut c = cache().write().await;
        c.insert(cache_key, (compiled.clone(), chrono::Utc::now()));
    }

    // Also persist to DB for durability (best-effort)
    let cursor = chrono::Utc::now().to_rfc3339();
    let json_str = serde_json::to_string(&compiled).unwrap_or_default();
    match &state.db {
        DbPool::Postgres(pool) => {
            let _ = sqlx::query("INSERT INTO knowledge_cache (tenant_id, scope, compiled_json, cursor, updated_at) VALUES ($1,'all',$2,$3,NOW()) ON CONFLICT (tenant_id, scope) DO UPDATE SET compiled_json=$2, cursor=$3, updated_at=NOW()").bind(&tenant).bind(&json_str).bind(&cursor).execute(pool).await;
        }
        DbPool::Sqlite(pool) => {
            let _ = sqlx::query("INSERT OR REPLACE INTO knowledge_cache (tenant_id, scope, compiled_json, cursor, updated_at) VALUES (?,?,?, ?, ?)").bind(&tenant).bind("all").bind(&json_str).bind(&cursor).bind(cursor.clone()).execute(pool).await;
        }
    }

    Ok(Json(
        serde_json::json!({"success": true, "cached": false, "data": compiled, "cursor": cursor}),
    ))
}

async fn compile_folder(
    state: &Arc<AppState>,
    mailbox_id: uuid::Uuid,
    folder: &str,
    limit: i64,
    snippet_len: usize,
) -> Value {
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, from_addr, subject, snippet, is_read, has_attachments, created_at FROM messages WHERE mailbox_id=$1 AND folder=$2 ORDER BY created_at DESC LIMIT $3")
                .bind(mailbox_id).bind(folder).bind(limit).fetch_all(pool).await.unwrap_or_default();
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<uuid::Uuid,_>("id").to_string(),
                "from": row.get::<String,_>("from_addr"),
                "subject": row.get::<Option<String>,_>("subject").unwrap_or_default(),
                "snippet": row.get::<Option<String>,_>("snippet").unwrap_or_default().chars().take(snippet_len).collect::<String>(),
                "is_read": row.try_get::<bool,_>("is_read").unwrap_or_else(|_| row.try_get::<i32,_>("is_read").map(|i| i!=0).unwrap_or(false)),
                "at": row.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at").map(|d| d.to_rfc3339()).unwrap_or_else(|_| row.try_get::<String,_>("created_at").unwrap_or_default()),
            })).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, from_addr, subject, snippet, is_read, created_at FROM messages WHERE mailbox_id=? AND folder=? ORDER BY created_at DESC LIMIT ?")
                .bind(mailbox_id.to_string()).bind(folder).bind(limit).fetch_all(pool).await.unwrap_or_default();
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<String,_>("id"),
                "from": row.get::<String,_>("from_addr"),
                "subject": row.get::<Option<String>,_>("subject").unwrap_or_default(),
                "snippet": row.get::<Option<String>,_>("snippet").unwrap_or_default().chars().take(snippet_len).collect::<String>(),
                "is_read": row.get::<i32,_>("is_read")!=0,
                "at": row.get::<String,_>("created_at"),
            })).collect()
        }
    };
    let total: i64 = match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE mailbox_id=$1 AND folder=$2")
                .bind(mailbox_id)
                .bind(folder)
                .fetch_one(pool)
                .await
                .unwrap_or(0)
        }
        DbPool::Sqlite(pool) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE mailbox_id=? AND folder=?")
                .bind(mailbox_id.to_string())
                .bind(folder)
                .fetch_one(pool)
                .await
                .unwrap_or(0)
        }
    };
    serde_json::json!({"total": total, "top": rows})
}

async fn compile_calendar(state: &Arc<AppState>, mailbox_id: &str, limit: i64) -> Value {
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, calendar, title, start_at, end_at FROM calendar_events WHERE mailbox_id=$1 ORDER BY start_at ASC LIMIT $2").bind(mailbox_id).bind(limit).fetch_all(pool).await.unwrap_or_default();
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<uuid::Uuid,_>("id").to_string(), "calendar": row.get::<String,_>("calendar"), "title": row.get::<String,_>("title"), "start_at": row.get::<String,_>("start_at")})).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, calendar, title, start_at, end_at FROM calendar_events WHERE mailbox_id=? ORDER BY start_at ASC LIMIT ?").bind(mailbox_id).bind(limit).fetch_all(pool).await.unwrap_or_default();
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<String,_>("id"), "calendar": row.get::<String,_>("calendar"), "title": row.get::<String,_>("title"), "start_at": row.get::<String,_>("start_at")})).collect()
        }
    };
    serde_json::json!({"next": rows})
}

async fn compile_overview(state: &Arc<AppState>, mailbox_id: uuid::Uuid) -> Value {
    let (total, unread, today) = match &state.db {
        DbPool::Postgres(pool) => {
            let t: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE mailbox_id=$1")
                .bind(mailbox_id)
                .fetch_one(pool)
                .await
                .unwrap_or(0);
            let u: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM messages WHERE mailbox_id=$1 AND folder='Inbox' AND is_read=false",
            )
            .bind(mailbox_id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
            let d: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM messages WHERE mailbox_id=$1 AND created_at >= NOW() - INTERVAL '1 day'",
            )
            .bind(mailbox_id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
            (t, u, d)
        }
        DbPool::Sqlite(pool) => {
            let t: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE mailbox_id=?")
                .bind(mailbox_id.to_string())
                .fetch_one(pool)
                .await
                .unwrap_or(0);
            let u: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM messages WHERE mailbox_id=? AND folder='Inbox' AND is_read=0",
            )
            .bind(mailbox_id.to_string())
            .fetch_one(pool)
            .await
            .unwrap_or(0);
            let d: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE mailbox_id=? AND datetime(created_at) >= datetime('now','-1 day')")
                .bind(mailbox_id.to_string())
                .fetch_one(pool)
                .await
                .unwrap_or(0);
            (t, u, d)
        }
    };
    serde_json::json!({"total": total, "unread_inbox": unread, "today": today})
}

async fn compile_threads_needing(state: &Arc<AppState>, mailbox_id: uuid::Uuid) -> Value {
    // reuse threads needing follow-up via simple query: last 5 threads
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query(
                "SELECT id, subject FROM threads WHERE mailbox_id=$1 ORDER BY last_message_at DESC LIMIT 5",
            )
            .bind(mailbox_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<uuid::Uuid,_>("id").to_string(), "subject": row.get::<Option<String>,_>("subject")})).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query(
                "SELECT id, subject FROM threads WHERE mailbox_id=? ORDER BY last_message_at DESC LIMIT 5",
            )
            .bind(mailbox_id.to_string())
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<String,_>("id"), "subject": row.get::<Option<String>,_>("subject")})).collect()
        }
    };
    serde_json::json!(rows)
}

/// Mailbox-scoped compiler used by capability MCP. Unlike the legacy user
/// route, every query and cache key carries both tenant and mailbox context;
/// no model-supplied identity is accepted.
pub async fn compile_for_context(
    state: &Arc<AppState>,
    context: &ExecutionContext,
    budget: usize,
) -> Result<Value, StatusCode> {
    let budget = budget.clamp(1, 20_000);
    let cache_key = format!(
        "mcp-v2:{}:{}:{}:{}",
        context.tenant_id,
        context.mailbox_id,
        budget,
        context.scopes.join(",")
    );
    {
        let cached = cache().read().await;
        if let Some((value, created_at)) = cached.get(&cache_key) {
            if (chrono::Utc::now() - *created_at).num_seconds() < 30 {
                return Ok(serde_json::json!({
                    "success": true,
                    "cached": true,
                    "data": value,
                }));
            }
        }
    }

    let inbox = compile_folder_for_context(state, context, "Inbox", 5).await?;
    let sent = compile_folder_for_context(state, context, "Sent", 5).await?;
    let drafts = compile_folder_for_context(state, context, "Drafts", 5).await?;
    let overview = compile_overview_for_context(state, context).await?;
    let threads = compile_threads_for_context(state, context).await?;
    let generated_at = chrono::Utc::now();
    let compiled = serde_json::json!({
        "tenant_id": context.tenant_id,
        "mailbox_id": context.mailbox_id,
        "budget": budget,
        "generated_at": generated_at.to_rfc3339(),
        "overview": overview,
        "inbox": inbox,
        "sent": sent,
        "drafts": drafts,
        "threads": threads,
    });

    let mut value = compiled;
    if value.to_string().len() > budget * 4 {
        if let Some(items) = value.get_mut("inbox").and_then(|v| v.get_mut("top")).and_then(Value::as_array_mut) {
            items.truncate(2);
        }
    }
    let now = chrono::Utc::now();
    cache().write().await.insert(cache_key, (value.clone(), now));

    let scope = format!("mcp-v2:mailbox:{}", context.mailbox_id);
    let json = serde_json::to_string(&value).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let cursor = now.to_rfc3339();
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO knowledge_cache (tenant_id, scope, compiled_json, cursor, updated_at) VALUES ($1,$2,$3,$4,NOW()) ON CONFLICT (tenant_id, scope) DO UPDATE SET compiled_json=$3, cursor=$4, updated_at=NOW()")
                .bind(context.tenant_id.to_string())
                .bind(&scope)
                .bind(&json)
                .bind(&cursor)
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT OR REPLACE INTO knowledge_cache (tenant_id, scope, compiled_json, cursor, updated_at) VALUES (?,?,?,?,?)")
                .bind(context.tenant_id.to_string())
                .bind(&scope)
                .bind(&json)
                .bind(&cursor)
                .bind(&cursor)
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }

    Ok(serde_json::json!({
        "success": true,
        "cached": false,
        "data": value,
        "cursor": cursor,
    }))
}

async fn compile_folder_for_context(
    state: &Arc<AppState>,
    context: &ExecutionContext,
    folder: &str,
    limit: i64,
) -> Result<Value, StatusCode> {
    let rows = match &state.db {
        DbPool::Postgres(pool) => sqlx::query("SELECT id, from_addr, subject, snippet FROM messages WHERE tenant_id=$1 AND mailbox_id=$2 AND folder=$3 ORDER BY created_at DESC LIMIT $4")
            .bind(context.tenant_id)
            .bind(context.mailbox_id)
            .bind(folder)
            .bind(limit)
            .fetch_all(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .into_iter()
            .map(|row| serde_json::json!({
                "id": row.get::<Uuid, _>("id").to_string(),
                "from": row.get::<String, _>("from_addr"),
                "subject": crate::mcp_limits::sanitize_for_ai(&row.get::<Option<String>, _>("subject").unwrap_or_default()),
                "snippet": crate::mcp_limits::sanitize_for_ai(&row.get::<Option<String>, _>("snippet").unwrap_or_default()),
            }))
            .collect::<Vec<_>>(),
        DbPool::Sqlite(pool) => sqlx::query("SELECT id, from_addr, subject, snippet FROM messages WHERE tenant_id=? AND mailbox_id=? AND folder=? ORDER BY created_at DESC LIMIT ?")
            .bind(context.tenant_id.to_string())
            .bind(context.mailbox_id.to_string())
            .bind(folder)
            .bind(limit)
            .fetch_all(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .into_iter()
            .map(|row| serde_json::json!({
                "id": row.get::<String, _>("id"),
                "from": row.get::<String, _>("from_addr"),
                "subject": crate::mcp_limits::sanitize_for_ai(&row.get::<Option<String>, _>("subject").unwrap_or_default()),
                "snippet": crate::mcp_limits::sanitize_for_ai(&row.get::<Option<String>, _>("snippet").unwrap_or_default()),
            }))
            .collect::<Vec<_>>(),
    };
    Ok(serde_json::json!({"folder": folder, "top": rows}))
}

async fn compile_overview_for_context(
    state: &Arc<AppState>,
    context: &ExecutionContext,
) -> Result<Value, StatusCode> {
    let (total, unread) = match &state.db {
        DbPool::Postgres(pool) => {
            let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM messages WHERE tenant_id=$1 AND mailbox_id=$2")
                .bind(context.tenant_id)
                .bind(context.mailbox_id)
                .fetch_one(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let unread = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM messages WHERE tenant_id=$1 AND mailbox_id=$2 AND folder='Inbox' AND is_read=false")
                .bind(context.tenant_id)
                .bind(context.mailbox_id)
                .fetch_one(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            (total, unread)
        }
        DbPool::Sqlite(pool) => {
            let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM messages WHERE tenant_id=? AND mailbox_id=?")
                .bind(context.tenant_id.to_string())
                .bind(context.mailbox_id.to_string())
                .fetch_one(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let unread = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM messages WHERE tenant_id=? AND mailbox_id=? AND folder='Inbox' AND is_read=0")
                .bind(context.tenant_id.to_string())
                .bind(context.mailbox_id.to_string())
                .fetch_one(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            (total, unread)
        }
    };
    Ok(serde_json::json!({"total": total, "unread_inbox": unread}))
}

async fn compile_threads_for_context(
    state: &Arc<AppState>,
    context: &ExecutionContext,
) -> Result<Value, StatusCode> {
    let rows = match &state.db {
        DbPool::Postgres(pool) => sqlx::query("SELECT id, subject FROM threads WHERE tenant_id=$1 AND mailbox_id=$2 ORDER BY last_message_at DESC LIMIT 5")
            .bind(context.tenant_id)
            .bind(context.mailbox_id)
            .fetch_all(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .into_iter()
            .map(|row| serde_json::json!({
                "id": row.get::<Uuid, _>("id").to_string(),
                "subject": crate::mcp_limits::sanitize_optional(row.get::<Option<String>, _>("subject")),
            }))
            .collect::<Vec<_>>(),
        DbPool::Sqlite(pool) => sqlx::query("SELECT id, subject FROM threads WHERE tenant_id=? AND mailbox_id=? ORDER BY last_message_at DESC LIMIT 5")
            .bind(context.tenant_id.to_string())
            .bind(context.mailbox_id.to_string())
            .fetch_all(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .into_iter()
            .map(|row| serde_json::json!({
                "id": row.get::<String, _>("id"),
                "subject": crate::mcp_limits::sanitize_optional(row.get::<Option<String>, _>("subject")),
            }))
            .collect::<Vec<_>>(),
    };
    Ok(Value::Array(rows))
}
