use std::sync::Arc;
use axum::{extract::{State, Path, Query}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use chrono::Utc;
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;
use aivory_mail_core::validation;

pub async fn list(State(state): State<Arc<AppState>>, Query(params): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let domain_filter = params.get("domain_id").and_then(|v| v.as_str());
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let q = if let Some(did) = domain_filter {
                let uid = Uuid::parse_str(did).map_err(|_| StatusCode::BAD_REQUEST)?;
                sqlx::query("SELECT id, address, display_name, is_catch_all, domain_id, created_at FROM mailboxes WHERE domain_id=$1 ORDER BY address")
                    .bind(uid).fetch_all(pool).await
            } else {
                sqlx::query("SELECT id, address, display_name, is_catch_all, domain_id, created_at FROM mailboxes ORDER BY address")
                    .fetch_all(pool).await
            };
            let r = q.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({
                "id": row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("id").unwrap_or_default()),
                "address": row.get::<String,_>("address"),
                "display_name": row.get::<Option<String>,_>("display_name"),
                "is_catch_all": row.try_get::<bool,_>("is_catch_all").unwrap_or_else(|_| row.try_get::<i32,_>("is_catch_all").map(|i| i!=0).unwrap_or(false)),
                "domain_id": row.try_get::<Uuid,_>("domain_id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("domain_id").unwrap_or_default()),
            })).collect()
        }
        DbPool::Sqlite(pool) => {
            let q = if let Some(did) = domain_filter {
                sqlx::query("SELECT id, address, display_name, is_catch_all, domain_id, created_at FROM mailboxes WHERE domain_id=? ORDER BY address")
                    .bind(did).fetch_all(pool).await
            } else {
                sqlx::query("SELECT id, address, display_name, is_catch_all, domain_id, created_at FROM mailboxes ORDER BY address")
                    .fetch_all(pool).await
            };
            let r = q.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<String,_>("id"),
                "address": row.get::<String,_>("address"),
                "display_name": row.get::<Option<String>,_>("display_name"),
                "is_catch_all": row.get::<i32,_>("is_catch_all") != 0,
                "domain_id": row.get::<String,_>("domain_id"),
            })).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn create(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let address = body.get("address").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    validation::validate_email(address).map_err(|_| StatusCode::BAD_REQUEST)?;
    let norm = validation::normalize_email(address);
    let domain_part = validation::extract_domain(&norm).ok_or(StatusCode::BAD_REQUEST)?;
    let display_name = body.get("display_name").and_then(|v| v.as_str()).map(|s| s.to_string());
    let is_catch_all = body.get("is_catch_all").and_then(|v| v.as_bool()).unwrap_or(false);
    let forward_to = body.get("forward_to").and_then(|v| v.as_str()).map(|s| s.to_string());
    let password = body.get("password").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty());
    if let Some(p) = password {
        if p.len() < 8 { return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"success": false, "error": "Password must be at least 8 characters"})))); }
    }
    let password_hash = password.map(aivory_mail_core::password::hash_password);
    let id = Uuid::new_v4();

    // find domain id
    let domain_id: Uuid = match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT id FROM domains WHERE lower(domain)=lower($1) LIMIT 1")
                .bind(&domain_part).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|r| r.get::<Uuid,_>("id")).ok_or(StatusCode::BAD_REQUEST)?
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id FROM domains WHERE lower(domain)=lower(?) LIMIT 1")
                .bind(&domain_part).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let s: String = row.ok_or(StatusCode::BAD_REQUEST)?.get("id");
            Uuid::parse_str(&s).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        }
    };

    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO mailboxes (id, tenant_id, domain_id, address, display_name, is_catch_all, forward_to, password_hash, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,NOW())")
                .bind(id).bind(Uuid::nil()).bind(domain_id).bind(&norm).bind(&display_name).bind(is_catch_all).bind(&forward_to).bind(&password_hash)
                .execute(pool).await.map_err(|e| { tracing::error!("insert mailbox: {}", e); StatusCode::CONFLICT })?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO mailboxes (id, tenant_id, domain_id, address, display_name, is_catch_all, forward_to, password_hash, created_at) VALUES (?,?,?,?,?,?,?,?,?)")
                .bind(id.to_string()).bind(Uuid::nil().to_string()).bind(domain_id.to_string()).bind(&norm).bind(&display_name).bind(if is_catch_all {1}else{0}).bind(&forward_to).bind(&password_hash).bind(Utc::now().to_rfc3339())
                .execute(pool).await.map_err(|_| StatusCode::CONFLICT)?;
        }
    }

    // Cloudflare: create routing rule
    if state.config.is_cloudflare() && state.config.cf_api_token.is_some() && state.config.cf_zone_id.is_some() {
        let client = crate::mail::cloudflare::CfClient::new(state.config.cf_api_token.clone().unwrap());
        let zone = state.config.cf_zone_id.clone().unwrap();
        let worker = std::env::var("CF_EMAIL_WORKER_NAME").unwrap_or_else(|_| "aivory-mail".into());
        let _ = client.create_routing_rule(&zone, &norm, &worker).await;
    }

    Ok((StatusCode::CREATED, Json(serde_json::json!({"success": true, "data": {"id": id, "address": norm}}))))
}

pub async fn get_one(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let val: Option<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT id, address, display_name FROM mailboxes WHERE id=$1").bind(uid).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|r| serde_json::json!({"id": r.get::<Uuid,_>("id").to_string(), "address": r.get::<String,_>("address")}))
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id, address FROM mailboxes WHERE id=?").bind(uid.to_string()).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|r| serde_json::json!({"id": r.get::<String,_>("id"), "address": r.get::<String,_>("address")}))
        }
    };
    val.map(|v| Json(serde_json::json!({"success": true, "data": v}))).ok_or(StatusCode::NOT_FOUND)
}

pub async fn update(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    if let Some(name) = body.get("display_name").and_then(|v| v.as_str()) {
        match &state.db {
            DbPool::Postgres(pool) => { sqlx::query("UPDATE mailboxes SET display_name=$1 WHERE id=$2").bind(name).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            DbPool::Sqlite(pool) => { sqlx::query("UPDATE mailboxes SET display_name=? WHERE id=?").bind(name).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        }
    }
    if let Some(pw) = body.get("password").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if pw.len() < 8 { return Err(StatusCode::BAD_REQUEST); }
        let hash = aivory_mail_core::password::hash_password(pw);
        match &state.db {
            DbPool::Postgres(pool) => { sqlx::query("UPDATE mailboxes SET password_hash=$1 WHERE id=$2").bind(&hash).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            DbPool::Sqlite(pool) => { sqlx::query("UPDATE mailboxes SET password_hash=? WHERE id=?").bind(&hash).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn remove(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("DELETE FROM mailboxes WHERE id=$1").bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("DELETE FROM mailboxes WHERE id=?").bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}
