use std::sync::Arc;
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::sync::RwLock;
use axum::{extract::{State, Query}, Json, http::StatusCode};
use serde_json::Value;
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

static CACHE: OnceLock<RwLock<HashMap<String, (Value, chrono::DateTime<chrono::Utc>)>>> = OnceLock::new();
fn cache() -> &'static RwLock<HashMap<String, ValueWithTime>> { CACHE.get_or_init(|| RwLock::new(HashMap::new())) }
type ValueWithTime = (Value, chrono::DateTime<chrono::Utc>);

pub async fn compile(State(state): State<Arc<AppState>>, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let budget: usize = q.get("budget").and_then(|v| v.as_u64()).unwrap_or(4000) as usize;
    let tenant = q.get("tenant_id").or_else(|| q.get("tenant")).and_then(|v| v.as_str()).unwrap_or("default").to_string();
    let cache_key = format!("{}:{}", tenant, budget);
    // Try cache 30s
    {
        let c = cache().read().await;
        if let Some((v, t)) = c.get(&cache_key) {
            if (chrono::Utc::now() - *t).num_seconds() < 30 {
                return Ok(Json(serde_json::json!({"success": true, "cached": true, "data": v})));
            }
        }
    }

    // Compile all scopes in parallel-ish (sequential for sqlite)
    let inbox = compile_folder(&state, "Inbox", 3, 300).await;
    let sent = compile_folder(&state, "Sent", 3, 200).await;
    let drafts = compile_folder(&state, "Drafts", 3, 200).await;
    let trash = compile_folder(&state, "Trash", 2, 150).await;
    let spam = compile_folder(&state, "Spam", 2, 150).await;
    let calendar = compile_calendar(&state, 5).await;
    let overview = compile_overview(&state).await;
    let threads_crawl = compile_threads_needing(&state).await;

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
        if let Some(arr) = compiled.get_mut("inbox").and_then(|v| v.get_mut("top")) .and_then(|v| v.as_array_mut()) {
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
        DbPool::Postgres(pool) => { let _ = sqlx::query("INSERT INTO knowledge_cache (tenant_id, scope, compiled_json, cursor, updated_at) VALUES ($1,'all',$2,$3,NOW()) ON CONFLICT (tenant_id, scope) DO UPDATE SET compiled_json=$2, cursor=$3, updated_at=NOW()").bind(&tenant).bind(&json_str).bind(&cursor).execute(pool).await; }
        DbPool::Sqlite(pool) => { let _ = sqlx::query("INSERT OR REPLACE INTO knowledge_cache (tenant_id, scope, compiled_json, cursor, updated_at) VALUES (?,?,?, ?, ?)").bind(&tenant).bind("all").bind(&json_str).bind(&cursor).bind(cursor.clone()).execute(pool).await; }
    }

    Ok(Json(serde_json::json!({"success": true, "cached": false, "data": compiled, "cursor": cursor})))
}

async fn compile_folder(state: &Arc<AppState>, folder: &str, limit: i64, snippet_len: usize) -> Value {
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, from_addr, subject, snippet, is_read, has_attachments, created_at FROM messages WHERE folder=$1 ORDER BY created_at DESC LIMIT $2")
                .bind(folder).bind(limit).fetch_all(pool).await.unwrap_or_default();
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<uuid::Uuid,_>("id").to_string(),
                "from": row.get::<String,_>("from_addr"),
                "subject": row.get::<Option<String>,_>("subject").unwrap_or_default(),
                "snippet": row.get::<Option<String>,_>("snippet").unwrap_or_default().chars().take(snippet_len).collect::<String>(),
                "is_read": row.get::<bool,_>("is_read"),
                "at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
            })).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, from_addr, subject, snippet, is_read, created_at FROM messages WHERE folder=? ORDER BY created_at DESC LIMIT ?")
                .bind(folder).bind(limit).fetch_all(pool).await.unwrap_or_default();
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
        DbPool::Postgres(pool) => sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder=$1").bind(folder).fetch_one(pool).await.unwrap_or(0),
        DbPool::Sqlite(pool) => sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder=?").bind(folder).fetch_one(pool).await.unwrap_or(0),
    };
    serde_json::json!({"total": total, "top": rows})
}

async fn compile_calendar(state: &Arc<AppState>, limit: i64) -> Value {
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, calendar, title, start_at, end_at FROM calendar_events ORDER BY start_at ASC LIMIT $1").bind(limit).fetch_all(pool).await.unwrap_or_default();
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<uuid::Uuid,_>("id").to_string(), "calendar": row.get::<String,_>("calendar"), "title": row.get::<String,_>("title"), "start_at": row.get::<String,_>("start_at")})).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, calendar, title, start_at, end_at FROM calendar_events ORDER BY start_at ASC LIMIT ?").bind(limit).fetch_all(pool).await.unwrap_or_default();
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<String,_>("id"), "calendar": row.get::<String,_>("calendar"), "title": row.get::<String,_>("title"), "start_at": row.get::<String,_>("start_at")})).collect()
        }
    };
    serde_json::json!({"next": rows})
}

async fn compile_overview(state: &Arc<AppState>) -> Value {
    let (total, unread, today) = match &state.db {
        DbPool::Postgres(pool) => {
            let t: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages").fetch_one(pool).await.unwrap_or(0);
            let u: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder='Inbox' AND is_read=false").fetch_one(pool).await.unwrap_or(0);
            let d: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE created_at >= NOW() - INTERVAL '1 day'").fetch_one(pool).await.unwrap_or(0);
            (t,u,d)
        }
        DbPool::Sqlite(pool) => {
            let t: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages").fetch_one(pool).await.unwrap_or(0);
            let u: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder='Inbox' AND is_read=0").fetch_one(pool).await.unwrap_or(0);
            let d: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE datetime(created_at) >= datetime('now','-1 day')").fetch_one(pool).await.unwrap_or(0);
            (t,u,d)
        }
    };
    serde_json::json!({"total": total, "unread_inbox": unread, "today": today})
}

async fn compile_threads_needing(state: &Arc<AppState>) -> Value {
    // reuse threads needing follow-up via simple query: last 5 threads
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, subject FROM threads ORDER BY last_message_at DESC LIMIT 5").fetch_all(pool).await.unwrap_or_default();
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<uuid::Uuid,_>("id").to_string(), "subject": row.get::<Option<String>,_>("subject")})).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, subject FROM threads ORDER BY last_message_at DESC LIMIT 5").fetch_all(pool).await.unwrap_or_default();
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<String,_>("id"), "subject": row.get::<Option<String>,_>("subject")})).collect()
        }
    };
    serde_json::json!(rows)
}
