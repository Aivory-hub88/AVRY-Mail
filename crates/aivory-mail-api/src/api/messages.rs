use std::sync::Arc;
use axum::{extract::{State, Path, Query}, Json, http::StatusCode, body::Body, response::Response};
use serde_json::Value;
use uuid::Uuid;
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

pub async fn list(State(state): State<Arc<AppState>>, Query(params): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = params.get("mailbox_id").and_then(|v| v.as_str());
    let folder = params.get("folder").and_then(|v| v.as_str()).unwrap_or("Inbox");
    let search = params.get("search").and_then(|v| v.as_str());
    let page: i64 = params.get("page").and_then(|v| v.as_i64()).unwrap_or(1);
    let per_page: i64 = params.get("per_page").and_then(|v| v.as_i64()).unwrap_or(20).min(100);
    let offset = (page - 1) * per_page;

    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let mut q = String::from("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, created_at FROM messages WHERE folder=$1");
            let mut args: Vec<String> = vec![folder.to_string()];
            if let Some(mid) = mailbox_id { q.push_str(" AND mailbox_id=$2"); args.push(mid.to_string()); }
            // search
            if let Some(s) = search { q.push_str(&format!(" AND (subject ILIKE '%{}%' OR snippet ILIKE '%{}%')", s, s)); }
            q.push_str(" ORDER BY created_at DESC LIMIT ");
            q.push_str(&per_page.to_string());
            q.push_str(" OFFSET ");
            q.push_str(&offset.to_string());

            // Use dynamic but safe-ish: for MVP we branch
            let r = if let Some(mid) = mailbox_id {
                let uid = Uuid::parse_str(mid).map_err(|_| StatusCode::BAD_REQUEST)?;
                if search.is_some() {
                    // include search
                    let s = search.unwrap();
                    sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, created_at FROM messages WHERE folder=$1 AND mailbox_id=$2 AND (subject ILIKE $3 OR snippet ILIKE $3) ORDER BY created_at DESC LIMIT $4 OFFSET $5")
                        .bind(folder).bind(uid).bind(format!("%{}%", s)).bind(per_page).bind(offset)
                        .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                } else {
                    sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, created_at FROM messages WHERE folder=$1 AND mailbox_id=$2 ORDER BY created_at DESC LIMIT $3 OFFSET $4")
                        .bind(folder).bind(uid).bind(per_page).bind(offset)
                        .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                }
            } else {
                sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, created_at FROM messages WHERE folder=$1 ORDER BY created_at DESC LIMIT $2 OFFSET $3")
                    .bind(folder).bind(per_page).bind(offset)
                    .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            };
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<Uuid,_>("id").to_string(),
                "from": row.get::<String,_>("from_addr"),
                "from_name": row.get::<Option<String>,_>("from_name"),
                "subject": row.get::<Option<String>,_>("subject"),
                "snippet": row.get::<Option<String>,_>("snippet"),
                "folder": row.get::<String,_>("folder"),
                "is_read": row.get::<bool,_>("is_read"),
                "is_starred": row.get::<bool,_>("is_starred"),
                "has_attachments": row.get::<bool,_>("has_attachments"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
            })).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = if let Some(mid) = mailbox_id {
                if let Some(s) = search {
                    sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, created_at FROM messages WHERE folder=? AND mailbox_id=? AND (subject LIKE ? OR snippet LIKE ?) ORDER BY created_at DESC LIMIT ? OFFSET ?")
                        .bind(folder).bind(mid).bind(format!("%{}%", s)).bind(format!("%{}%", s)).bind(per_page).bind(offset)
                        .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                } else {
                    sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, created_at FROM messages WHERE folder=? AND mailbox_id=? ORDER BY created_at DESC LIMIT ? OFFSET ?")
                        .bind(folder).bind(mid).bind(per_page).bind(offset)
                        .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                }
            } else {
                sqlx::query("SELECT id, from_addr, subject, snippet, folder, is_read, is_starred, has_attachments, created_at FROM messages WHERE folder=? ORDER BY created_at DESC LIMIT ? OFFSET ?")
                    .bind(folder).bind(per_page).bind(offset)
                    .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            };
            r.into_iter().map(|row| {
                let id: String = row.get("id");
                serde_json::json!({
                    "id": id,
                    "from": row.get::<String,_>("from_addr"),
                    "subject": row.get::<Option<String>,_>("subject"),
                    "snippet": row.get::<Option<String>,_>("snippet"),
                    "folder": row.get::<String,_>("folder"),
                    "is_read": row.get::<i32,_>("is_read") != 0,
                    "has_attachments": row.get::<i32,_>("has_attachments") != 0,
                    "created_at": row.get::<String,_>("created_at"),
                })
            }).collect()
        }
    };

    Ok(Json(serde_json::json!({"success": true, "data": rows, "page": page, "per_page": per_page})))
}

pub async fn get_one(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let val: Option<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT id, from_addr, from_name, to_addrs, cc_addrs, subject, snippet, body_text, body_html, folder, is_read, headers_json, created_at FROM messages WHERE id=$1")
                .bind(uid).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|r| serde_json::json!({
                "id": r.get::<Uuid,_>("id").to_string(),
                "from": r.get::<String,_>("from_addr"),
                "from_name": r.get::<Option<String>,_>("from_name"),
                "to": r.get::<String,_>("to_addrs"),
                "subject": r.get::<Option<String>,_>("subject"),
                "snippet": r.get::<Option<String>,_>("snippet"),
                "body_text": r.get::<Option<String>,_>("body_text"),
                "body_html": r.get::<Option<String>,_>("body_html"),
                "folder": r.get::<String,_>("folder"),
                "is_read": r.get::<bool,_>("is_read"),
                "headers": r.get::<Option<serde_json::Value>,_>("headers_json"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
            }))
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id, from_addr, subject, body_text, body_html, folder, is_read, created_at FROM messages WHERE id=?")
                .bind(uid.to_string()).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|r| serde_json::json!({
                "id": r.get::<String,_>("id"),
                "from": r.get::<String,_>("from_addr"),
                "subject": r.get::<Option<String>,_>("subject"),
                "body_text": r.get::<Option<String>,_>("body_text"),
                "body_html": r.get::<Option<String>,_>("body_html"),
                "folder": r.get::<String,_>("folder"),
                "is_read": r.get::<i32,_>("is_read") != 0,
                "created_at": r.get::<String,_>("created_at"),
            }))
        }
    };
    // mark as read side-effect
    if val.is_some() {
        match &state.db {
            DbPool::Postgres(pool) => { let _ = sqlx::query("UPDATE messages SET is_read=true WHERE id=$1").bind(uid).execute(pool).await; }
            DbPool::Sqlite(pool) => { let _ = sqlx::query("UPDATE messages SET is_read=1 WHERE id=?").bind(uid.to_string()).execute(pool).await; }
        }
    }
    val.map(|v| Json(serde_json::json!({"success": true, "data": v}))).ok_or(StatusCode::NOT_FOUND)
}

pub async fn remove(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("UPDATE messages SET folder='Trash' WHERE id=$1").bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("UPDATE messages SET folder='Trash' WHERE id=?").bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn mark_read(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let is_read = body.get("is_read").and_then(|v| v.as_bool()).unwrap_or(true);
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("UPDATE messages SET is_read=$1 WHERE id=$2").bind(is_read).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("UPDATE messages SET is_read=? WHERE id=?").bind(if is_read{1}else{0}).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn move_message(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let folder = body.get("folder").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let allowed = ["Inbox","Sent","Drafts","Spam","Trash","Archive"];
    if !allowed.contains(&folder) { return Err(StatusCode::BAD_REQUEST); }
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("UPDATE messages SET folder=$1 WHERE id=$2").bind(folder).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("UPDATE messages SET folder=? WHERE id=?").bind(folder).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true, "folder": folder})))
}

pub async fn download_attachment(State(state): State<Arc<AppState>>, Path((id, att_id)): Path<(String, String)>) -> Result<Response<Body>, StatusCode> {
    let _msg_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let att_uuid = Uuid::parse_str(&att_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let (r2_key, filename, ct): (String, String, String) = match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT r2_key, filename, content_type FROM attachments WHERE id=$1")
                .bind(att_uuid).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
            (row.get("r2_key"), row.get("filename"), row.get("content_type"))
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT r2_key, filename, content_type FROM attachments WHERE id=?")
                .bind(att_uuid.to_string()).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
            (row.get("r2_key"), row.get("filename"), row.get("content_type"))
        }
    };
    let data = state.store.get(&r2_key).await.map_err(|_| StatusCode::NOT_FOUND)?;
    let headers = [
        (axum::http::header::CONTENT_TYPE, ct),
        (axum::http::header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", filename)),
    ];
    let mut resp = Response::new(Body::from(data));
    for (k,v) in headers { resp.headers_mut().insert(k, v.parse().unwrap()); }
    Ok(resp)
}
