use std::sync::Arc;
use axum::{extract::{State, Path, Query}, Json, http::StatusCode, body::Body, response::Response};
use serde_json::Value;
use uuid::Uuid;
use sqlx::Row;
use crate::api::{AppState, audit};
use aivory_mail_storage::db::DbPool;

pub async fn list(State(state): State<Arc<AppState>>, Query(params): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = params.get("mailbox_id").and_then(|v| v.as_str());
    let folder = params.get("folder").and_then(|v| v.as_str()).unwrap_or("Inbox");
    let search = params.get("search").and_then(|v| v.as_str());
    let page: i64 = params.get("page").and_then(|v| v.as_i64()).unwrap_or(1);
    let per_page: i64 = params.get("per_page").and_then(|v| v.as_i64()).unwrap_or(20).min(100);
    let offset = (page - 1) * per_page;
    let is_snoozed_folder = folder.eq_ignore_ascii_case("Snoozed");
    let now_str = chrono::Utc::now().to_rfc3339();

    // snoozed handling: Snoozed is virtual folder (snoozed_until > now). Inbox etc exclude snoozed.
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = if is_snoozed_folder {
                // Snoozed view: snoozed_until is future
                if let Some(mid) = mailbox_id {
                    let uid = Uuid::parse_str(mid).map_err(|_| StatusCode::BAD_REQUEST)?;
                    if let Some(s) = search {
                        sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE snoozed_until IS NOT NULL AND snoozed_until > $1::timestamptz AND mailbox_id=$2 AND (subject ILIKE $3 OR snippet ILIKE $3) ORDER BY snoozed_until ASC LIMIT $4 OFFSET $5")
                            .bind(chrono::DateTime::parse_from_rfc3339(&now_str).map(|d| d.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now())).bind(uid).bind(format!("%{}%", s)).bind(per_page).bind(offset)
                            .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                    } else {
                        sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE snoozed_until IS NOT NULL AND snoozed_until > $1::timestamptz AND mailbox_id=$2 ORDER BY snoozed_until ASC LIMIT $3 OFFSET $4")
                            .bind(chrono::DateTime::parse_from_rfc3339(&now_str).map(|d| d.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now())).bind(uid).bind(per_page).bind(offset)
                            .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                    }
                } else {
                    if let Some(s) = search {
                        sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE snoozed_until IS NOT NULL AND snoozed_until > $1::timestamptz AND (subject ILIKE $2 OR snippet ILIKE $2) ORDER BY snoozed_until ASC LIMIT $3 OFFSET $4")
                            .bind(chrono::DateTime::parse_from_rfc3339(&now_str).map(|d| d.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now())).bind(format!("%{}%", s)).bind(per_page).bind(offset)
                            .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                    } else {
                        sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE snoozed_until IS NOT NULL AND snoozed_until > $1::timestamptz ORDER BY snoozed_until ASC LIMIT $2 OFFSET $3")
                            .bind(chrono::DateTime::parse_from_rfc3339(&now_str).map(|d| d.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now())).bind(per_page).bind(offset)
                            .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                    }
                }
            } else if let Some(mid) = mailbox_id {
                let uid = Uuid::parse_str(mid).map_err(|_| StatusCode::BAD_REQUEST)?;
                if search.is_some() {
                    let s = search.unwrap();
                    sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE folder=$1 AND mailbox_id=$2 AND (snoozed_until IS NULL OR snoozed_until <= $3::timestamptz) AND (subject ILIKE $4 OR snippet ILIKE $4) ORDER BY created_at DESC LIMIT $5 OFFSET $6")
                        .bind(folder).bind(uid).bind(chrono::DateTime::parse_from_rfc3339(&now_str).map(|d| d.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now())).bind(format!("%{}%", s)).bind(per_page).bind(offset)
                        .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                } else {
                    sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE folder=$1 AND mailbox_id=$2 AND (snoozed_until IS NULL OR snoozed_until <= $3::timestamptz) ORDER BY created_at DESC LIMIT $4 OFFSET $5")
                        .bind(folder).bind(uid).bind(chrono::DateTime::parse_from_rfc3339(&now_str).map(|d| d.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now())).bind(per_page).bind(offset)
                        .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                }
            } else {
                if search.is_some() {
                    let s = search.unwrap();
                    sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE folder=$1 AND (snoozed_until IS NULL OR snoozed_until <= $2::timestamptz) AND (subject ILIKE $3 OR snippet ILIKE $3) ORDER BY created_at DESC LIMIT $4 OFFSET $5")
                        .bind(folder).bind(chrono::DateTime::parse_from_rfc3339(&now_str).map(|d| d.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now())).bind(format!("%{}%", s)).bind(per_page).bind(offset)
                        .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                } else {
                    sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE folder=$1 AND (snoozed_until IS NULL OR snoozed_until <= $2::timestamptz) ORDER BY created_at DESC LIMIT $3 OFFSET $4")
                        .bind(folder).bind(chrono::DateTime::parse_from_rfc3339(&now_str).map(|d| d.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now())).bind(per_page).bind(offset)
                        .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                }
            };
            r.into_iter().map(|row| {
                let snoozed: Option<chrono::DateTime<chrono::Utc>> = row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("snoozed_until");
                serde_json::json!({
                    "id": row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("id").unwrap_or_default()),
                    "from": row.get::<String,_>("from_addr"),
                    "from_name": row.get::<Option<String>,_>("from_name"),
                    "subject": row.get::<Option<String>,_>("subject"),
                    "snippet": row.get::<Option<String>,_>("snippet"),
                    "folder": row.get::<String,_>("folder"),
                    "is_read": row.try_get::<bool,_>("is_read").unwrap_or_else(|_| row.try_get::<i32,_>("is_read").map(|i| i!=0).unwrap_or(false)),
                    "is_starred": row.try_get::<bool,_>("is_starred").unwrap_or_else(|_| row.try_get::<i32,_>("is_starred").map(|i| i!=0).unwrap_or(false)),
                    "has_attachments": row.try_get::<bool,_>("has_attachments").unwrap_or_else(|_| row.try_get::<i32,_>("has_attachments").map(|i| i!=0).unwrap_or(false)),
                    "snoozed_until": snoozed.map(|d| d.to_rfc3339()),
                    "created_at": row.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at").map(|d| d.to_rfc3339()).unwrap_or_else(|_| row.try_get::<String,_>("created_at").unwrap_or_default()),
                })
            }).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = if is_snoozed_folder {
                if let Some(mid) = mailbox_id {
                    if let Some(s) = search {
                        sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE snoozed_until IS NOT NULL AND snoozed_until > ? AND mailbox_id=? AND (subject LIKE ? OR snippet LIKE ?) ORDER BY snoozed_until ASC LIMIT ? OFFSET ?")
                            .bind(&now_str).bind(mid).bind(format!("%{}%", s)).bind(format!("%{}%", s)).bind(per_page).bind(offset)
                            .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                    } else {
                        sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE snoozed_until IS NOT NULL AND snoozed_until > ? AND mailbox_id=? ORDER BY snoozed_until ASC LIMIT ? OFFSET ?")
                            .bind(&now_str).bind(mid).bind(per_page).bind(offset)
                            .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                    }
                } else {
                    if let Some(s) = search {
                        sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE snoozed_until IS NOT NULL AND snoozed_until > ? AND (subject LIKE ? OR snippet LIKE ?) ORDER BY snoozed_until ASC LIMIT ? OFFSET ?")
                            .bind(&now_str).bind(format!("%{}%", s)).bind(format!("%{}%", s)).bind(per_page).bind(offset)
                            .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                    } else {
                        sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE snoozed_until IS NOT NULL AND snoozed_until > ? ORDER BY snoozed_until ASC LIMIT ? OFFSET ?")
                            .bind(&now_str).bind(per_page).bind(offset)
                            .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                    }
                }
            } else if let Some(mid) = mailbox_id {
                if let Some(s) = search {
                    sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE folder=? AND mailbox_id=? AND (snoozed_until IS NULL OR snoozed_until <= ? OR snoozed_until='') AND (subject LIKE ? OR snippet LIKE ?) ORDER BY created_at DESC LIMIT ? OFFSET ?")
                        .bind(folder).bind(mid).bind(&now_str).bind(format!("%{}%", s)).bind(format!("%{}%", s)).bind(per_page).bind(offset)
                        .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                } else {
                    sqlx::query("SELECT id, from_addr, from_name, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE folder=? AND mailbox_id=? AND (snoozed_until IS NULL OR snoozed_until <= ? OR snoozed_until='') ORDER BY created_at DESC LIMIT ? OFFSET ?")
                        .bind(folder).bind(mid).bind(&now_str).bind(per_page).bind(offset)
                        .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                }
            } else {
                if search.is_some() {
                    let s = search.unwrap();
                    sqlx::query("SELECT id, from_addr, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE folder=? AND (snoozed_until IS NULL OR snoozed_until <= ? OR snoozed_until='') AND (subject LIKE ? OR snippet LIKE ?) ORDER BY created_at DESC LIMIT ? OFFSET ?")
                        .bind(folder).bind(&now_str).bind(format!("%{}%", s)).bind(format!("%{}%", s)).bind(per_page).bind(offset)
                        .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                } else {
                    sqlx::query("SELECT id, from_addr, subject, snippet, folder, is_read, is_starred, has_attachments, snoozed_until, created_at FROM messages WHERE folder=? AND (snoozed_until IS NULL OR snoozed_until <= ? OR snoozed_until='') ORDER BY created_at DESC LIMIT ? OFFSET ?")
                        .bind(folder).bind(&now_str).bind(per_page).bind(offset)
                        .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                }
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
                    "snoozed_until": row.get::<Option<String>,_>("snoozed_until"),
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
            let row = sqlx::query("SELECT id, from_addr, from_name, to_addrs, cc_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, has_attachments, thread_id, headers_json, created_at FROM messages WHERE id=$1")
                .bind(uid).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if let Some(r) = row {
                let msg_id: Uuid = r.get("id");
                // fetch attachments
                let atts = sqlx::query("SELECT id, filename, content_type, size_bytes FROM attachments WHERE message_id=$1").bind(msg_id).fetch_all(pool).await.unwrap_or_default();
                let att_json: Vec<serde_json::Value> = atts.into_iter().map(|a| serde_json::json!({"id": a.get::<Uuid,_>("id").to_string(), "filename": a.get::<String,_>("filename"), "content_type": a.get::<String,_>("content_type"), "size_bytes": a.get::<i32,_>("size_bytes")})).collect();
                Some(serde_json::json!({
                    "id": msg_id.to_string(),
                    "from": r.get::<String,_>("from_addr"),
                    "from_name": r.get::<Option<String>,_>("from_name"),
                    "to": r.get::<String,_>("to_addrs"),
                    "cc": r.get::<String,_>("cc_addrs"),
                    "subject": r.get::<Option<String>,_>("subject"),
                    "snippet": r.get::<Option<String>,_>("snippet"),
                    "body_text": r.get::<Option<String>,_>("body_text"),
                    "body_html": r.get::<Option<String>,_>("body_html"),
                    "folder": r.get::<String,_>("folder"),
                    "is_read": r.get::<bool,_>("is_read"),
                    "is_starred": r.get::<bool,_>("is_starred"),
                    "has_attachments": r.get::<bool,_>("has_attachments"),
                    "thread_id": r.get::<Option<Uuid>,_>("thread_id").map(|u| u.to_string()),
                    "headers": r.get::<Option<serde_json::Value>,_>("headers_json"),
                    "attachments": att_json,
                    "created_at": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
                }))
            } else { None }
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id, from_addr, subject, body_text, body_html, folder, is_read, is_starred, has_attachments, thread_id, created_at FROM messages WHERE id=?")
                .bind(uid.to_string()).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if let Some(r) = row {
                let sid: String = r.get("id");
                let atts = sqlx::query("SELECT id, filename, content_type, size_bytes FROM attachments WHERE message_id=?").bind(&sid).fetch_all(pool).await.unwrap_or_default();
                let att_json: Vec<serde_json::Value> = atts.into_iter().map(|a| serde_json::json!({"id": a.get::<String,_>("id"), "filename": a.get::<String,_>("filename"), "content_type": a.get::<String,_>("content_type"), "size_bytes": a.get::<i32,_>("size_bytes")})).collect();
                Some(serde_json::json!({
                    "id": sid,
                    "from": r.get::<String,_>("from_addr"),
                    "subject": r.get::<Option<String>,_>("subject"),
                    "body_text": r.get::<Option<String>,_>("body_text"),
                    "body_html": r.get::<Option<String>,_>("body_html"),
                    "folder": r.get::<String,_>("folder"),
                    "is_read": r.get::<i32,_>("is_read") != 0,
                    "is_starred": r.get::<i32,_>("is_starred") != 0,
                    "has_attachments": r.get::<i32,_>("has_attachments") != 0,
                    "thread_id": r.get::<Option<String>,_>("thread_id"),
                    "attachments": att_json,
                    "created_at": r.get::<String,_>("created_at"),
                }))
            } else { None }
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
    let db = state.db.clone(); tokio::spawn(async move { audit::log(&db, if is_read {"email.read"} else {"email.unread"}, None, None, None, Some(&id), None).await; });
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn move_message(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let folder = body.get("folder").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    if folder.is_empty() || folder.len() > 80 { return Err(StatusCode::BAD_REQUEST); }
    // system folders + custom folders allowed; Snoozed is virtual via snoozed_until, not folder
    let system = ["Inbox","Sent","Drafts","Spam","Trash","Archive","Snoozed"];
    if system.contains(&folder) && folder=="Snoozed" { return Err(StatusCode::BAD_REQUEST); }
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("UPDATE messages SET folder=$1 WHERE id=$2").bind(folder).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("UPDATE messages SET folder=? WHERE id=?").bind(folder).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    let db = state.db.clone(); let fid = folder.to_string(); let mid = id.clone(); tokio::spawn(async move { audit::log(&db, "email.move", None, None, None, Some(&mid), Some(serde_json::json!({"folder": fid}))).await; });
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

pub async fn snooze(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let snoozed_str = body.get("snoozed_until").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    // validate future ISO
    let dt = chrono::DateTime::parse_from_rfc3339(snoozed_str).map_err(|_| StatusCode::BAD_REQUEST)?.with_timezone(&chrono::Utc);
    if dt <= chrono::Utc::now() { return Err(StatusCode::BAD_REQUEST); }
    let iso = dt.to_rfc3339();
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("UPDATE messages SET snoozed_until=$1 WHERE id=$2").bind(dt).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("UPDATE messages SET snoozed_until=? WHERE id=?").bind(&iso).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    let db = state.db.clone(); let mid = id.clone(); let iso2 = iso.clone(); tokio::spawn(async move { audit::log(&db, "email.snooze", None, None, None, Some(&mid), Some(serde_json::json!({"snoozed_until": iso2}))).await; });
    Ok(Json(serde_json::json!({"success": true, "snoozed_until": iso})))
}

pub async fn unsnooze(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("UPDATE messages SET snoozed_until=NULL WHERE id=$1").bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("UPDATE messages SET snoozed_until=NULL WHERE id=?").bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    let db = state.db.clone(); let mid = id.clone(); tokio::spawn(async move { audit::log(&db, "email.unsnooze", None, None, None, Some(&mid), None).await; });
    Ok(Json(serde_json::json!({"success": true})))
}
