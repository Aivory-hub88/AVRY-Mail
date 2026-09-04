use std::sync::Arc;
use axum::{extract::{State, Query}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

pub async fn log(db: &DbPool, action: &str, actor_id: Option<&str>, target_id: Option<&str>, mailbox_id: Option<&str>, message_id: Option<&str>, metadata: Option<Value>) {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let meta_str = metadata.map(|v| v.to_string());
    match db {
        DbPool::Postgres(pool) => {
            let _ = sqlx::query("INSERT INTO audit_logs (id, actor_id, target_id, mailbox_id, message_id, action, metadata, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,NOW())")
                .bind(Uuid::parse_str(&id).unwrap_or(Uuid::nil())).bind(actor_id).bind(target_id).bind(mailbox_id).bind(message_id).bind(action).bind(&meta_str).execute(pool).await;
        }
        DbPool::Sqlite(pool) => {
            let _ = sqlx::query("INSERT INTO audit_logs (id, actor_id, target_id, mailbox_id, message_id, action, metadata, created_at) VALUES (?,?,?,?,?,?,?,?)")
                .bind(&id).bind(actor_id).bind(target_id).bind(mailbox_id).bind(message_id).bind(action).bind(&meta_str).bind(&now).execute(pool).await;
        }
    }
}

pub async fn list(State(state): State<Arc<AppState>>, Query(params): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let limit: i64 = params.get("limit").and_then(|v| v.as_i64()).unwrap_or(50).min(200);
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, actor_id, target_id, mailbox_id, message_id, action, metadata, created_at FROM audit_logs ORDER BY created_at DESC LIMIT $1")
                .bind(limit).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({
                "id": row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("id").unwrap_or_default()),
                "action": row.get::<String,_>("action"),
                "actor_id": row.get::<Option<String>,_>("actor_id"),
                "mailbox_id": row.get::<Option<String>,_>("mailbox_id"),
                "message_id": row.get::<Option<String>,_>("message_id"),
                "metadata": row.get::<Option<String>,_>("metadata").and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()),
                "created_at": row.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at").map(|d| d.to_rfc3339()).unwrap_or_else(|_| row.try_get::<String,_>("created_at").unwrap_or_default()),
            })).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, actor_id, target_id, mailbox_id, message_id, action, metadata, created_at FROM audit_logs ORDER BY created_at DESC LIMIT ?")
                .bind(limit).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<String,_>("id"),
                "action": row.get::<String,_>("action"),
                "actor_id": row.get::<Option<String>,_>("actor_id"),
                "mailbox_id": row.get::<Option<String>,_>("mailbox_id"),
                "message_id": row.get::<Option<String>,_>("message_id"),
                "metadata": row.get::<Option<String>,_>("metadata").and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()),
                "created_at": row.get::<String,_>("created_at"),
            })).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}
