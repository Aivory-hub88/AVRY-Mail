use std::sync::Arc;
use axum::{extract::{State, Query}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use sqlx::Row;
use crate::api::{AppState, audit};
use aivory_mail_storage::db::DbPool;

pub async fn list(State(state): State<Arc<AppState>>, Query(params): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, email, display_name, blocked, last_seen_at FROM contacts WHERE tenant_id='default' ORDER BY last_seen_at DESC LIMIT 100")
                .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<Uuid,_>("id").to_string(),
                "email": row.get::<String,_>("email"),
                "display_name": row.get::<String,_>("display_name"),
                "blocked": row.get::<bool,_>("blocked"),
                "last_seen_at": row.get::<chrono::DateTime<chrono::Utc>,_>("last_seen_at").to_rfc3339(),
            })).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, email, display_name, blocked, last_seen_at FROM contacts WHERE tenant_id='default' ORDER BY last_seen_at DESC LIMIT 100")
                .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<String,_>("id"),
                "email": row.get::<String,_>("email"),
                "display_name": row.get::<String,_>("display_name"),
                "blocked": row.get::<i32,_>("blocked") != 0,
                "last_seen_at": row.get::<String,_>("last_seen_at"),
            })).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn block(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let email = body.get("email").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?.to_lowercase();
    let display_name = body.get("display_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let id = Uuid::new_v4();
    // upsert contact as blocked
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO contacts (id, tenant_id, email, display_name, blocked, last_seen_at, created_at) VALUES ($1,'default',$2,$3,true,NOW(),NOW()) ON CONFLICT (tenant_id, email) DO UPDATE SET blocked=true, display_name=EXCLUDED.display_name, last_seen_at=NOW()")
                .bind(id).bind(&email).bind(&display_name).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            // also create filter rule trash
            let fid = Uuid::new_v4();
            let criteria = serde_json::json!({"from": email}).to_string();
            let action = serde_json::json!({"move": "Trash"}).to_string();
            let _ = sqlx::query("INSERT INTO mail_filters (id, tenant_id, name, criteria_json, action_json, enabled, created_at) VALUES ($1,'default',$2,$3,$4,true,NOW()) ON CONFLICT DO NOTHING")
                .bind(fid).bind(format!("Block {}", email)).bind(&criteria).bind(&action).execute(pool).await;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO contacts (id, tenant_id, email, display_name, blocked, last_seen_at, created_at) VALUES (?,?,?,?,?,?,?) ON CONFLICT(tenant_id, email) DO UPDATE SET blocked=1, display_name=excluded.display_name, last_seen_at=excluded.last_seen_at")
                .bind(id.to_string()).bind("default").bind(&email).bind(&display_name).bind(&now).bind(&now).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let fid = Uuid::new_v4().to_string();
            let criteria = serde_json::json!({"from": email}).to_string();
            let action = serde_json::json!({"move": "Trash"}).to_string();
            let _ = sqlx::query("INSERT OR IGNORE INTO mail_filters (id, tenant_id, name, criteria_json, action_json, enabled, created_at) VALUES (?,?,?,?,?,?,?)")
                .bind(&fid).bind("default").bind(format!("Block {}", email)).bind(&criteria).bind(&action).bind(1).bind(&now).execute(pool).await;
        }
    }
    let db2 = state.db.clone(); let em = email.clone(); tokio::spawn(async move { audit::log(&db2, "contact.block", None, Some(&em), None, None, None).await; });
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn upsert_from_address(db: &DbPool, email: &str, display_name: &str) {
    let email = email.to_lowercase();
    if email.is_empty() || !email.contains('@') { return; }
    let now = chrono::Utc::now().to_rfc3339();
    let id = Uuid::new_v4();
    match db {
        DbPool::Postgres(pool) => {
            let _ = sqlx::query("INSERT INTO contacts (id, tenant_id, email, display_name, blocked, last_seen_at, created_at) VALUES ($1,'default',$2,$3,false,NOW(),NOW()) ON CONFLICT (tenant_id, email) DO UPDATE SET display_name=CASE WHEN contacts.display_name='' THEN EXCLUDED.display_name ELSE contacts.display_name END, last_seen_at=NOW()")
                .bind(id).bind(&email).bind(display_name).execute(pool).await;
        }
        DbPool::Sqlite(pool) => {
            let _ = sqlx::query("INSERT INTO contacts (id, tenant_id, email, display_name, blocked, last_seen_at, created_at) VALUES (?,?,?,?,?,?,?) ON CONFLICT(tenant_id, email) DO UPDATE SET display_name=CASE WHEN display_name='' THEN excluded.display_name ELSE display_name END, last_seen_at=excluded.last_seen_at")
                .bind(id.to_string()).bind("default").bind(&email).bind(display_name).bind(0).bind(&now).bind(&now).execute(pool).await;
        }
    }
}
