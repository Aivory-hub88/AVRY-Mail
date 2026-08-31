use std::sync::Arc;
use axum::{extract::State, Json, http::{HeaderMap, StatusCode}, body::Bytes};
use serde_json::Value;
use sqlx::Row;
use sha2::{Sha256, Digest};
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

fn hash_key(raw: &str) -> String { let mut h=Sha256::new(); h.update(raw.as_bytes()); format!("{:x}", h.finalize()) }

async fn validate_api_key(state: &Arc<AppState>, headers: &HeaderMap, query_key: Option<String>) -> bool {
    let mut raw: Option<String> = None;
    if let Some(auth) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if auth.to_lowercase().starts_with("bearer ") { raw = Some(auth[7..].trim().to_string()); }
    }
    if raw.is_none() { raw = query_key; }
    if raw.is_none() { raw = headers.get("x-api-key").and_then(|v| v.to_str().ok()).map(|s| s.to_string()); }
    let Some(k) = raw else { return false; };
    let hash = hash_key(&k);
    match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id FROM api_keys WHERE key_hash=$1 LIMIT 1").bind(&hash).fetch_optional(pool).await;
            matches!(r, Ok(Some(_)))
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id FROM api_keys WHERE key_hash=? LIMIT 1").bind(&hash).fetch_optional(pool).await;
            matches!(r, Ok(Some(_)))
        }
    }
}

pub async fn mcp_handler(State(state): State<Arc<AppState>>, headers: HeaderMap, body: Bytes) -> Result<Json<Value>, StatusCode> {
    // Allow internal token bypass for Cerveau, or API key
    let is_internal = headers.get("x-internal-token").and_then(|v| v.to_str().ok()) == Some(&state.config.internal_token) ||
        headers.get("x-cerveau-internal-secret").and_then(|v| v.to_str().ok()).is_some();
    if !is_internal {
        // Check query ?api_key= via header already, but we need query parsing — for now just check Bearer
        let ok = validate_api_key(&state, &headers, None).await;
        if !ok { return Err(StatusCode::UNAUTHORIZED); }
    }
    let v: Value = serde_json::from_slice(&body).unwrap_or(serde_json::json!({}));
    let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = v.get("id").cloned().unwrap_or(serde_json::json!(1));
    let result = match method {
        "initialize" => serde_json::json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}}, "serverInfo":{"name":"aivory-mail-mcp","version":"0.1.0"}}),
        "tools/list" => serde_json::json!({"tools": [
            {"name":"search_mail","description":"Hybrid search mail (vector+FTS) — use instead of list scan","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"folder":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]}},
            {"name":"get_inbox_overview","description":"1-call inbox stats","inputSchema":{"type":"object","properties":{}}},
            {"name":"get_thread_memory","description":"Budgeted thread context for LLM","inputSchema":{"type":"object","properties":{"thread_id":{"type":"string"},"budget":{"type":"integer"}},"required":["thread_id"]}},
            {"name":"get_knowledge_compile","description":"Auto-compiled knowledge for all folders","inputSchema":{"type":"object","properties":{"budget":{"type":"integer"}},"required":[]}},
            {"name":"send_mail","description":"Send email via Aivory Mail","inputSchema":{"type":"object","properties":{"from":{"type":"string"},"to":{"type":"array"},"subject":{"type":"string"},"text":{"type":"string"}},"required":["from","to","subject"]}}
        ]}),
        "tools/call" => {
            let name = v.get("params").and_then(|p| p.get("name")).and_then(|n| n.as_str()).unwrap_or("");
            match name {
                "search_mail" => {
                    let q = v.get("params").and_then(|p| p.get("arguments")).and_then(|a| a.get("query")).and_then(|s| s.as_str()).unwrap_or("invoice");
                    serde_json::json!({"content":[{"type":"text","text": format!("search_mail stub for query: {} — call GET /v1/search?q={} for real data", q, q)}]})
                }
                _ => serde_json::json!({"content":[{"type":"text","text": format!("tool {} called", name)}]})
            }
        }
        _ => serde_json::json!({"error": format!("unknown method {}", method)}),
    };
    Ok(Json(serde_json::json!({"jsonrpc":"2.0","id": id, "result": result})))
}
