use std::sync::Arc;
use axum::{extract::{State, Path, Query}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;
use aivory_mail_core::validation;

pub async fn list(State(state): State<Arc<AppState>>, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = q.get("mailbox_id").and_then(|v| v.as_str());
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = if let Some(mid) = mailbox_id {
                let uid = Uuid::parse_str(mid).map_err(|_| StatusCode::BAD_REQUEST)?;
                sqlx::query("SELECT id, mailbox_id, alias_email, display_name, is_default FROM send_as_aliases WHERE mailbox_id=$1 ORDER BY is_default DESC, created_at DESC").bind(uid).fetch_all(pool).await
            } else {
                sqlx::query("SELECT id, mailbox_id, alias_email, display_name, is_default FROM send_as_aliases ORDER BY created_at DESC").fetch_all(pool).await
            };
            let r = r.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({"id": row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("id").unwrap_or_default()), "mailbox_id": row.try_get::<Uuid,_>("mailbox_id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("mailbox_id").unwrap_or_default()), "alias_email": row.get::<String,_>("alias_email"), "display_name": row.get::<String,_>("display_name"), "is_default": row.try_get::<bool,_>("is_default").unwrap_or_else(|_| row.try_get::<i32,_>("is_default").map(|i| i!=0).unwrap_or(false))})).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = if let Some(mid) = mailbox_id {
                sqlx::query("SELECT id, mailbox_id, alias_email, display_name, is_default FROM send_as_aliases WHERE mailbox_id=? ORDER BY is_default DESC").bind(mid).fetch_all(pool).await
            } else {
                sqlx::query("SELECT id, mailbox_id, alias_email, display_name, is_default FROM send_as_aliases ORDER BY created_at DESC").fetch_all(pool).await
            };
            let r = r.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<String,_>("id"), "mailbox_id": row.get::<String,_>("mailbox_id"), "alias_email": row.get::<String,_>("alias_email"), "display_name": row.get::<String,_>("display_name"), "is_default": row.get::<i32,_>("is_default")!=0})).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn create(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let mailbox_id_str = body.get("mailbox_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let mailbox_id = Uuid::parse_str(mailbox_id_str).map_err(|_| StatusCode::BAD_REQUEST)?;
    let alias_email = body.get("alias_email").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    validation::validate_email(alias_email).map_err(|_| StatusCode::BAD_REQUEST)?;
    let alias_email = validation::normalize_email(alias_email);
    let display_name = body.get("display_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let is_default = body.get("is_default").and_then(|v| v.as_bool()).unwrap_or(false);
    let id = Uuid::new_v4();
    if is_default {
        match &state.db {
            DbPool::Postgres(pool) => { let _ = sqlx::query("UPDATE send_as_aliases SET is_default=false WHERE mailbox_id=$1").bind(mailbox_id).execute(pool).await; }
            DbPool::Sqlite(pool) => { let _ = sqlx::query("UPDATE send_as_aliases SET is_default=0 WHERE mailbox_id=?").bind(mailbox_id.to_string()).execute(pool).await; }
        }
    }
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("INSERT INTO send_as_aliases (id, mailbox_id, alias_email, display_name, is_default, created_at) VALUES ($1,$2,$3,$4,$5,NOW())").bind(id).bind(mailbox_id).bind(&alias_email).bind(&display_name).bind(is_default).execute(pool).await.map_err(|_| StatusCode::CONFLICT)?; }
        DbPool::Sqlite(pool) => { sqlx::query("INSERT INTO send_as_aliases (id, mailbox_id, alias_email, display_name, is_default, created_at) VALUES (?,?,?,?,?,?)").bind(id.to_string()).bind(mailbox_id.to_string()).bind(&alias_email).bind(&display_name).bind(if is_default{1}else{0}).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await.map_err(|_| StatusCode::CONFLICT)?; }
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({"success": true, "data": {"id": id.to_string(), "mailbox_id": mailbox_id.to_string(), "alias_email": alias_email}}))))
}

pub async fn remove(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("DELETE FROM send_as_aliases WHERE id=$1").bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("DELETE FROM send_as_aliases WHERE id=?").bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}
