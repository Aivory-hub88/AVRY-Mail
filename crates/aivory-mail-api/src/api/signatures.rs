use std::sync::Arc;
use axum::{extract::{State, Path, Query}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

pub async fn list(State(state): State<Arc<AppState>>, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = q.get("mailbox_id").and_then(|v| v.as_str());
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = if let Some(mid) = mailbox_id {
                let uid = Uuid::parse_str(mid).map_err(|_| StatusCode::BAD_REQUEST)?;
                sqlx::query("SELECT id, mailbox_id, name, html, text, is_default, created_at FROM signatures WHERE mailbox_id=$1 ORDER BY is_default DESC, created_at DESC").bind(uid).fetch_all(pool).await
            } else {
                sqlx::query("SELECT id, mailbox_id, name, html, text, is_default, created_at FROM signatures ORDER BY created_at DESC").fetch_all(pool).await
            };
            let r = r.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<Uuid,_>("id").to_string(), "mailbox_id": row.get::<Uuid,_>("mailbox_id").to_string(), "name": row.get::<String,_>("name"), "html": row.get::<String,_>("html"), "text": row.get::<String,_>("text"), "is_default": row.get::<bool,_>("is_default")})).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = if let Some(mid) = mailbox_id {
                sqlx::query("SELECT id, mailbox_id, name, html, text, is_default FROM signatures WHERE mailbox_id=? ORDER BY is_default DESC").bind(mid).fetch_all(pool).await
            } else {
                sqlx::query("SELECT id, mailbox_id, name, html, text, is_default FROM signatures ORDER BY created_at DESC").fetch_all(pool).await
            };
            let r = r.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<String,_>("id"), "mailbox_id": row.get::<String,_>("mailbox_id"), "name": row.get::<String,_>("name"), "html": row.get::<String,_>("html"), "text": row.get::<String,_>("text"), "is_default": row.get::<i32,_>("is_default")!=0})).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn create(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let mailbox_id_str = body.get("mailbox_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let mailbox_id = Uuid::parse_str(mailbox_id_str).map_err(|_| StatusCode::BAD_REQUEST)?;
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("Default").to_string();
    let html = body.get("html").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let text = body.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let is_default = body.get("is_default").and_then(|v| v.as_bool()).unwrap_or(false);
    let id = Uuid::new_v4();
    if is_default {
        match &state.db {
            DbPool::Postgres(pool) => { let _ = sqlx::query("UPDATE signatures SET is_default=false WHERE mailbox_id=$1").bind(mailbox_id).execute(pool).await; }
            DbPool::Sqlite(pool) => { let _ = sqlx::query("UPDATE signatures SET is_default=0 WHERE mailbox_id=?").bind(mailbox_id.to_string()).execute(pool).await; }
        }
    }
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("INSERT INTO signatures (id, mailbox_id, name, html, text, is_default, created_at) VALUES ($1,$2,$3,$4,$5,$6,NOW())").bind(id).bind(mailbox_id).bind(&name).bind(&html).bind(&text).bind(is_default).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("INSERT INTO signatures (id, mailbox_id, name, html, text, is_default, created_at) VALUES (?,?,?,?,?,?,?)").bind(id.to_string()).bind(mailbox_id.to_string()).bind(&name).bind(&html).bind(&text).bind(if is_default{1}else{0}).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({"success": true, "data": {"id": id.to_string(), "mailbox_id": mailbox_id.to_string(), "name": name}}))))
}

pub async fn update(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let html = body.get("html").and_then(|v| v.as_str());
    let text = body.get("text").and_then(|v| v.as_str());
    let name = body.get("name").and_then(|v| v.as_str());
    let is_default = body.get("is_default").and_then(|v| v.as_bool());
    match &state.db {
        DbPool::Postgres(pool) => {
            if let Some(h) = html { sqlx::query("UPDATE signatures SET html=$1 WHERE id=$2").bind(h).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(t) = text { sqlx::query("UPDATE signatures SET text=$1 WHERE id=$2").bind(t).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(n) = name { sqlx::query("UPDATE signatures SET name=$1 WHERE id=$2").bind(n).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(d) = is_default { sqlx::query("UPDATE signatures SET is_default=$1 WHERE id=$2").bind(d).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        }
        DbPool::Sqlite(pool) => {
            if let Some(h) = html { sqlx::query("UPDATE signatures SET html=? WHERE id=?").bind(h).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(t) = text { sqlx::query("UPDATE signatures SET text=? WHERE id=?").bind(t).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(n) = name { sqlx::query("UPDATE signatures SET name=? WHERE id=?").bind(n).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(d) = is_default { sqlx::query("UPDATE signatures SET is_default=? WHERE id=?").bind(if d{1}else{0}).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn remove(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("DELETE FROM signatures WHERE id=$1").bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("DELETE FROM signatures WHERE id=?").bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}
