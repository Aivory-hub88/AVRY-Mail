use std::sync::Arc;
use axum::{extract::{State, Query, Path}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

pub async fn list(State(state): State<Arc<AppState>>, Query(params): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = params.get("mailbox_id").and_then(|v| v.as_str());
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let q = if let Some(mid) = mailbox_id {
                let uid = Uuid::parse_str(mid).map_err(|_| StatusCode::BAD_REQUEST)?;
                sqlx::query("SELECT id, mailbox_id, name, color, created_at FROM folders WHERE mailbox_id=$1 ORDER BY name").bind(uid).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            } else {
                sqlx::query("SELECT id, mailbox_id, name, color, created_at FROM folders ORDER BY name").fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            };
            q.into_iter().map(|r| serde_json::json!({
                "id": r.get::<Uuid,_>("id").to_string(),
                "mailbox_id": r.get::<Uuid,_>("mailbox_id").to_string(),
                "name": r.get::<String,_>("name"),
                "color": r.get::<String,_>("color"),
            })).collect()
        }
        DbPool::Sqlite(pool) => {
            let q = if let Some(mid) = mailbox_id {
                sqlx::query("SELECT id, mailbox_id, name, color FROM folders WHERE mailbox_id=? ORDER BY name").bind(mid).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            } else {
                sqlx::query("SELECT id, mailbox_id, name, color FROM folders ORDER BY name").fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            };
            q.into_iter().map(|r| serde_json::json!({
                "id": r.get::<String,_>("id"),
                "mailbox_id": r.get::<String,_>("mailbox_id"),
                "name": r.get::<String,_>("name"),
                "color": r.get::<String,_>("color"),
            })).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn create(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let mailbox_id = body.get("mailbox_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let name = body.get("name").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?.trim().to_string();
    if name.is_empty() || name.len() > 80 { return Err(StatusCode::BAD_REQUEST); }
    let color = body.get("color").and_then(|v| v.as_str()).unwrap_or("#006355").to_string();
    let id = Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();
    match &state.db {
        DbPool::Postgres(pool) => {
            let mid = Uuid::parse_str(mailbox_id).map_err(|_| StatusCode::BAD_REQUEST)?;
            sqlx::query("INSERT INTO folders (id, tenant_id, mailbox_id, name, color, created_at) VALUES ($1,'default',$2,$3,$4,NOW())")
                .bind(id).bind(mid).bind(&name).bind(&color).execute(pool).await.map_err(|_| StatusCode::CONFLICT)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO folders (id, tenant_id, mailbox_id, name, color, created_at) VALUES (?,?,?,?,?,?)")
                .bind(id.to_string()).bind("default").bind(mailbox_id).bind(&name).bind(&color).bind(&now).execute(pool).await.map_err(|_| StatusCode::CONFLICT)?;
        }
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({"success": true, "data": {"id": id.to_string(), "name": name, "color": color}}))))
}

pub async fn remove(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("DELETE FROM folders WHERE id=$1").bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("DELETE FROM folders WHERE id=?").bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}
