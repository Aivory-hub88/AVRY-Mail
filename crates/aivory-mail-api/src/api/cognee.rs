use std::sync::Arc;
use axum::{extract::{State, Query}, Json, http::StatusCode};
use serde_json::Value;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

pub async fn sync(State(state): State<Arc<AppState>>, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let since = q.get("since").and_then(|v| v.as_str());
    let limit: i64 = crate::api::query_i64(q.get("limit")).unwrap_or(100).min(500);
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = if let Some(s) = since {
                sqlx::query("SELECT id, from_addr, subject, snippet, created_at FROM messages WHERE created_at > $1::timestamptz ORDER BY created_at ASC LIMIT $2")
                    .bind(s).bind(limit).fetch_all(pool).await
            } else {
                sqlx::query("SELECT id, from_addr, subject, snippet, created_at FROM messages ORDER BY created_at DESC LIMIT $1")
                    .bind(limit).fetch_all(pool).await
            };
            let r = r.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| {
                use sqlx::Row;
                serde_json::json!({"id": row.get::<uuid::Uuid,_>("id").to_string(), "from": row.get::<String,_>("from_addr"), "subject": row.get::<Option<String>,_>("subject"), "at": row.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at").map(|d| d.to_rfc3339()).unwrap_or_else(|_| row.try_get::<String,_>("created_at").unwrap_or_default())})
            }).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = if let Some(s) = since {
                sqlx::query("SELECT id, from_addr, subject, snippet, created_at FROM messages WHERE datetime(created_at) > datetime(?) ORDER BY created_at ASC LIMIT ?")
                    .bind(s).bind(limit).fetch_all(pool).await
            } else {
                sqlx::query("SELECT id, from_addr, subject, snippet, created_at FROM messages ORDER BY created_at DESC LIMIT ?")
                    .bind(limit).fetch_all(pool).await
            };
            let r = r.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| {
                use sqlx::Row;
                serde_json::json!({"id": row.get::<String,_>("id"), "from": row.get::<String,_>("from_addr"), "subject": row.get::<Option<String>,_>("subject"), "at": row.get::<String,_>("created_at")})
            }).collect()
        }
    };
    let next_cursor = rows.last().and_then(|v| v.get("at").and_then(|a| a.as_str())).map(|s| s.to_string());
    Ok(Json(serde_json::json!({"success": true, "data": rows, "next_cursor": next_cursor, "hint": "incremental — Cerveau/Cognee-RS pulls with ?since=cursor, no full scan; vector embed in Cognee later"})))
}

pub async fn mcp_tools(State(_state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    Ok(Json(serde_json::json!({"success": true, "data": {
        "tools": [
            {"name":"search_mail","description":"Hybrid search mail (vector+FTS) — use instead of list scan","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"folder":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]}},
            {"name":"get_inbox_overview","description":"1-call inbox stats (unread, threads, today)","inputSchema":{"type":"object","properties":{}}},
            {"name":"get_thread_memory","description":"Budgeted thread context for LLM — no scan","inputSchema":{"type":"object","properties":{"thread_id":{"type":"string"},"budget":{"type":"integer"}},"required":["thread_id"]}},
            {"name":"list_mail","description":"Legacy paginated list — prefer search_mail","inputSchema":{"type":"object","properties":{"folder":{"type":"string"},"page":{"type":"integer"}}} },
            {"name":"get_thread_crawl","description":"Timeline + follow-up suggestion","inputSchema":{"type":"object","properties":{"thread_id":{"type":"string"}},"required":["thread_id"]}},
            {"name":"get_knowledge_compile","description":"Auto-compiled knowledge list for all folders — 1 call vs scan, use for agent overview","inputSchema":{"type":"object","properties":{"budget":{"type":"integer"},"tenant_id":{"type":"string"}},"required":[]}}
        ],
        "endpoint_base": "http://avry-mail:8095",
        "hint": "Cerveau/Cognee-RS: call search_mail with query, not list_mail loop"
    }})))
}
