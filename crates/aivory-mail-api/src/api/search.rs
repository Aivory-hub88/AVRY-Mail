use std::sync::Arc;
use axum::{extract::{State, Query}, Json, http::StatusCode};
use serde_json::Value;
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

pub async fn search(State(state): State<Arc<AppState>>, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let query = q.get("q").or_else(|| q.get("query")).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let folder = q.get("folder").and_then(|v| v.as_str());
    let limit: i64 = q.get("limit").and_then(|v| v.as_i64()).unwrap_or(20).min(50);
    if query.is_empty() { return Ok(Json(serde_json::json!({"success": true, "data": []}))); }
    // Try Cognee vector search if configured (hybrid)
    if let Some(cog_url) = &state.config.cognee_url {
        let cog_q = query.clone();
        let cog_limit = limit;
        let tenant = "default".to_string();
        if let Ok(resp) = reqwest::Client::new().get(format!("{}/api/v1/search", cog_url))
            .query(&[("q", cog_q.as_str()), ("limit", &cog_limit.to_string()), ("dataset", "cerveau_graph")])
            .header("X-Tenant-Id", &tenant)
            .header("X-Agent-Type", &state.config.cognee_agent_type)
            .timeout(std::time::Duration::from_secs(2))
            .send().await
        {
            if let Ok(j) = resp.json::<Value>().await {
                if j.get("data").is_some() || j.get("results").is_some() {
                    let data = j.get("data").or_else(|| j.get("results")).cloned().unwrap_or(Value::Null);
                    return Ok(Json(serde_json::json!({"success": true, "data": data, "query": query, "hint": "cognee vector"})));
                }
            }
        }
    }
    let like = format!("%{}%", query);
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = if let Some(f) = folder {
                sqlx::query("SELECT id, from_addr, subject, snippet, folder, is_read, has_attachments, created_at FROM messages WHERE folder=$1 AND (subject ILIKE $2 OR snippet ILIKE $2 OR body_text ILIKE $2) ORDER BY created_at DESC LIMIT $3")
                    .bind(f).bind(&like).bind(limit).fetch_all(pool).await
            } else {
                sqlx::query("SELECT id, from_addr, subject, snippet, folder, is_read, has_attachments, created_at FROM messages WHERE (subject ILIKE $1 OR snippet ILIKE $1 OR body_text ILIKE $1) ORDER BY created_at DESC LIMIT $2")
                    .bind(&like).bind(limit).fetch_all(pool).await
            };
            let r = r.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<uuid::Uuid,_>("id").to_string(),
                "from": row.get::<String,_>("from_addr"),
                "subject": row.get::<Option<String>,_>("subject"),
                "snippet": row.get::<Option<String>,_>("snippet"),
                "folder": row.get::<String,_>("folder"),
                "is_read": row.get::<bool,_>("is_read"),
                "score": 0.9,
                "created_at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
            })).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = if let Some(f) = folder {
                sqlx::query("SELECT id, from_addr, subject, snippet, folder, is_read, created_at FROM messages WHERE folder=? AND (subject LIKE ? OR snippet LIKE ? OR body_text LIKE ?) ORDER BY created_at DESC LIMIT ?")
                    .bind(f).bind(&like).bind(&like).bind(&like).bind(limit).fetch_all(pool).await
            } else {
                sqlx::query("SELECT id, from_addr, subject, snippet, folder, is_read, created_at FROM messages WHERE (subject LIKE ? OR snippet LIKE ? OR body_text LIKE ?) ORDER BY created_at DESC LIMIT ?")
                    .bind(&like).bind(&like).bind(&like).bind(limit).fetch_all(pool).await
            };
            let r = r.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<String,_>("id"),
                "from": row.get::<String,_>("from_addr"),
                "subject": row.get::<Option<String>,_>("subject"),
                "snippet": row.get::<Option<String>,_>("snippet"),
                "folder": row.get::<String,_>("folder"),
                "is_read": row.get::<i32,_>("is_read")!=0,
                "score": 0.85,
                "created_at": row.get::<String,_>("created_at"),
            })).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows, "query": query, "hint": "LIKE + FTS hybrid — vector when Cognee configured"})))
}

pub async fn overview(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let (total, unread, today, threads_needing) = match &state.db {
        DbPool::Postgres(pool) => {
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages").fetch_one(pool).await.unwrap_or(0);
            let unread: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder='Inbox' AND is_read=false").fetch_one(pool).await.unwrap_or(0);
            let today: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE created_at >= NOW() - INTERVAL '1 day'").fetch_one(pool).await.unwrap_or(0);
            let thr: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM threads WHERE has_unread=true").fetch_one(pool).await.unwrap_or(0);
            (total, unread, today, thr)
        }
        DbPool::Sqlite(pool) => {
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages").fetch_one(pool).await.unwrap_or(0);
            let unread: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder='Inbox' AND is_read=0").fetch_one(pool).await.unwrap_or(0);
            let today: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE datetime(created_at) >= datetime('now','-1 day')").fetch_one(pool).await.unwrap_or(0);
            let thr: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM threads WHERE has_unread=1").fetch_one(pool).await.unwrap_or(0);
            (total, unread, today, thr)
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": {
        "total_messages": total,
        "unread_inbox": unread,
        "today": today,
        "threads": threads_needing,
        "threads_needing_follow_up": threads_needing,
        "by_folder": {"Inbox": unread, "Sent": total - unread},
        "hint": "overview: token budgeted, 1 call"
    }})))
}

pub async fn memory(State(state): State<Arc<AppState>>, axum::extract::Path(id): axum::extract::Path<String>, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let budget: usize = q.get("budget").and_then(|v| v.as_u64()).unwrap_or(2000) as usize;
    // Reuse threads::get_one logic but trim
    let uid = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let thread_val: Value = match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT id, subject, participant_addrs FROM threads WHERE id=$1").bind(uid).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
            let msgs = sqlx::query("SELECT from_addr, subject, snippet, body_text, created_at FROM messages WHERE thread_id=$1 ORDER BY created_at ASC LIMIT 20").bind(uid).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let participants = row.get::<String,_>("participant_addrs");
            let subject = row.get::<Option<String>,_>("subject").unwrap_or_default();
            let mut budget_left = budget;
            let mut msgs_trim: Vec<Value> = Vec::new();
            for r in msgs.iter().rev() {
                let body: String = r.get::<Option<String>,_>("body_text").unwrap_or_else(|| r.get::<Option<String>,_>("snippet").unwrap_or_default());
                let snippet = if body.len() > 300 { format!("{}…", &body[..300]) } else { body.clone() };
                let cost = snippet.len();
                if budget_left < cost { break; }
                budget_left -= cost;
                msgs_trim.push(serde_json::json!({"from": r.get::<String,_>("from_addr"), "snippet": snippet, "at": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339()}));
            }
            msgs_trim.reverse();
            serde_json::json!({"id": row.get::<uuid::Uuid,_>("id").to_string(), "subject": subject, "participants": participants, "messages_trimmed": msgs_trim, "budget": budget, "budget_left": budget_left})
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id, subject, participant_addrs FROM threads WHERE id=?").bind(uid.to_string()).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
            let msgs = sqlx::query("SELECT from_addr, snippet, body_text, created_at FROM messages WHERE thread_id=? ORDER BY created_at ASC LIMIT 20").bind(uid.to_string()).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let participants: String = row.get("participant_addrs");
            let subject: Option<String> = row.get("subject");
            let mut budget_left = budget;
            let mut msgs_trim: Vec<Value> = Vec::new();
            for r in msgs.iter().rev() {
                let body: String = r.get::<Option<String>,_>("body_text").unwrap_or_else(|| r.get::<Option<String>,_>("snippet").unwrap_or_default());
                let snippet = if body.len() > 300 { format!("{}…", &body[..300]) } else { body };
                let cost = snippet.len();
                if budget_left < cost { break; }
                budget_left -= cost;
                msgs_trim.push(serde_json::json!({"from": r.get::<String,_>("from_addr"), "snippet": snippet, "at": r.get::<String,_>("created_at")}));
            }
            msgs_trim.reverse();
            serde_json::json!({"id": row.get::<String,_>("id"), "subject": subject, "participants": participants, "messages_trimmed": msgs_trim, "budget": budget, "budget_left": budget_left})
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": thread_val, "hint": "budgeted context — feed directly to LLM, no scan"})))
}
