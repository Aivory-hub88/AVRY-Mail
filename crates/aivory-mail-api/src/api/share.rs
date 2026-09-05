use std::sync::Arc;
use axum::{extract::{State, Path}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use jsonwebtoken::{encode, Header, EncodingKey};
use serde::{Serialize, Deserialize};
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

#[derive(Debug, Serialize, Deserialize)]
struct ShareClaims { mid: String, exp: usize }

pub async fn create_share(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let mid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    // verify message exists
    let exists = match &state.db {
        DbPool::Postgres(pool) => sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM messages WHERE id=$1").bind(mid).fetch_one(pool).await.unwrap_or(0) > 0,
        DbPool::Sqlite(pool) => sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM messages WHERE id=?").bind(mid.to_string()).fetch_one(pool).await.unwrap_or(0) > 0,
    };
    if !exists { return Err(StatusCode::NOT_FOUND); }
    let exp = (chrono::Utc::now() + chrono::Duration::days(7)).timestamp() as usize;
    let claims = ShareClaims { mid: mid.to_string(), exp };
    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(state.config.jwt_secret.as_bytes())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let base = std::env::var("PUBLIC_MAIL_URL").unwrap_or_else(|_| "http://localhost:3005".into());
    let url = format!("{}/share/{}?t={}", base, mid, token);
    Ok(Json(serde_json::json!({"success": true, "data": {"url": url, "token": token, "expires_at": exp}})))
}

pub async fn get_shared(State(state): State<Arc<AppState>>, Path(id): Path<String>, axum::extract::Query(params): axum::extract::Query<Value>) -> Result<Json<Value>, StatusCode> {
    let token = params.get("t").and_then(|v| v.as_str()).ok_or(StatusCode::UNAUTHORIZED)?;
    // verify token
    let claims = jsonwebtoken::decode::<ShareClaims>(token, &jsonwebtoken::DecodingKey::from_secret(state.config.jwt_secret.as_bytes()), &jsonwebtoken::Validation::default()).map_err(|_| StatusCode::UNAUTHORIZED)?;
    if claims.claims.mid != id { return Err(StatusCode::FORBIDDEN); }
    // fetch message (public, no auth)
    let mid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let val: Option<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT id, from_addr, subject, body_text, body_html, created_at FROM messages WHERE id=$1").bind(mid).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|r| {
                use sqlx::Row;
                serde_json::json!({"id": r.get::<Uuid,_>("id").to_string(), "from": r.get::<String,_>("from_addr"), "subject": r.get::<Option<String>,_>("subject"), "body_text": r.get::<Option<String>,_>("body_text"), "body_html": r.get::<Option<String>,_>("body_html"), "created_at": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339()})
            })
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id, from_addr, subject, body_text, body_html, created_at FROM messages WHERE id=?").bind(mid.to_string()).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|r| {
                use sqlx::Row;
                serde_json::json!({"id": r.get::<String,_>("id"), "from": r.get::<String,_>("from_addr"), "subject": r.get::<Option<String>,_>("subject"), "body_text": r.get::<Option<String>,_>("body_text"), "body_html": r.get::<Option<String>,_>("body_html"), "created_at": r.get::<String,_>("created_at")})
            })
        }
    };
    val.map(|v| Json(serde_json::json!({"success": true, "data": v}))).ok_or(StatusCode::NOT_FOUND)
}

// Star toggle
pub async fn toggle_star(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let mid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("UPDATE messages SET is_starred = NOT is_starred WHERE id=$1").bind(mid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("UPDATE messages SET is_starred = CASE is_starred WHEN 1 THEN 0 ELSE 1 END WHERE id=?").bind(mid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

// Drafts
pub async fn list_drafts(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, from_addr, to_addrs, subject, snippet, created_at FROM messages WHERE folder='Drafts' ORDER BY created_at DESC LIMIT 50").fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| {
                use sqlx::Row;
                serde_json::json!({"id": row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("id").unwrap_or_default()), "from": row.get::<String,_>("from_addr"), "subject": row.get::<Option<String>,_>("subject"), "snippet": row.get::<Option<String>,_>("snippet")})
            }).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, from_addr, subject FROM messages WHERE folder='Drafts' ORDER BY created_at DESC LIMIT 50").fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| { use sqlx::Row; serde_json::json!({"id": row.get::<String,_>("id"), "from": row.get::<String,_>("from_addr")})}).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn save_draft(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let id = body.get("id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()).unwrap_or(Uuid::new_v4());
    let from = body.get("from").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let to = body.get("to").cloned().unwrap_or(serde_json::json!([]));
    let subject = body.get("subject").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let text = body.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let html = body.get("html").and_then(|v| v.as_str()).map(|s| s.to_string());
    let to_str = serde_json::to_string(&to).unwrap();
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO messages (id, tenant_id, mailbox_id, message_id, from_addr, to_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, size_bytes, has_attachments, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'Drafts',true,false,0,false,NOW()) ON CONFLICT (id) DO UPDATE SET to_addrs=$6, subject=$7, snippet=$8, body_text=$9, body_html=$10")
                .bind(id).bind(Uuid::nil()).bind(Uuid::nil()).bind(format!("<draft-{}@aivory.mail>", id)).bind(&from).bind(&to_str).bind(&subject).bind(text.chars().take(80).collect::<String>()).bind(&text).bind(&html).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT OR REPLACE INTO messages (id, tenant_id, mailbox_id, message_id, from_addr, to_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, size_bytes, has_attachments, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id.to_string()).bind(Uuid::nil().to_string()).bind(Uuid::nil().to_string()).bind(format!("<draft-{}@aivory.mail>", id)).bind(&from).bind(&to_str).bind(&subject).bind(text.chars().take(80).collect::<String>()).bind(&text).bind(&html).bind("Drafts").bind(1).bind(0).bind(0).bind(0).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(Json(serde_json::json!({"success": true, "data": {"id": id.to_string()}})))
}
