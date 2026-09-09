use crate::api::{authz, AppState};
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
