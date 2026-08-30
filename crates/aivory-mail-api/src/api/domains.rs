use std::sync::Arc;
use axum::{extract::{State, Path}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use chrono::Utc;
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;
use aivory_mail_core::{validation, types::DomainStatus};

pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, domain, status, sending_subdomain, created_at FROM domains ORDER BY created_at DESC")
                .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| {
                let id: Uuid = row.get("id");
                serde_json::json!({"id": id, "domain": row.get::<String,_>("domain"), "status": row.get::<String,_>("status"), "sending_subdomain": row.get::<Option<String>,_>("sending_subdomain"), "created_at": row.get::<chrono::DateTime<Utc>,_>("created_at")})
            }).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, domain, status, sending_subdomain, created_at FROM domains ORDER BY created_at DESC")
                .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| {
                serde_json::json!({"id": row.get::<String,_>("id"), "domain": row.get::<String,_>("domain"), "status": row.get::<String,_>("status"), "sending_subdomain": row.get::<Option<String>,_>("sending_subdomain"), "created_at": row.get::<String,_>("created_at")})
            }).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn create(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let domain_raw = body.get("domain").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    validation::validate_domain(domain_raw).map_err(|_| StatusCode::BAD_REQUEST)?;
    let domain = validation::normalize_domain(domain_raw);
    let id = Uuid::new_v4();
    let tenant_id = body.get("tenant_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()).unwrap_or(Uuid::nil());

    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO domains (id, tenant_id, domain, status, dkim_selector, created_at) VALUES ($1,$2,$3,'Pending','aivory',NOW())")
                .bind(id).bind(tenant_id).bind(&domain).execute(pool).await.map_err(|e| { tracing::error!("insert domain: {}", e); StatusCode::CONFLICT })?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO domains (id, tenant_id, domain, status, dkim_selector, created_at) VALUES (?,?,?,?,?,?)")
                .bind(id.to_string()).bind(tenant_id.to_string()).bind(&domain).bind("Pending").bind("aivory").bind(Utc::now().to_rfc3339())
                .execute(pool).await.map_err(|_| StatusCode::CONFLICT)?;
        }
    }

    // If Cloudflare mode, try to auto-provision DNS
    if state.config.is_cloudflare() && state.config.cf_api_token.is_some() && state.config.cf_zone_id.is_some() {
        let client = crate::mail::cloudflare::CfClient::new(state.config.cf_api_token.clone().unwrap());
        let zone = state.config.cf_zone_id.clone().unwrap();
        let _ = client.enable_email_routing(&zone).await;
    }

    Ok((StatusCode::CREATED, Json(serde_json::json!({"success": true, "data": {"id": id, "domain": domain, "status": "Pending"}}))))
}

pub async fn get_one(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let val: Option<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT id, domain, status, dkim_selector, sending_subdomain, verified_at, created_at FROM domains WHERE id=$1")
                .bind(uid).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|r| serde_json::json!({"id": r.get::<Uuid,_>("id"), "domain": r.get::<String,_>("domain"), "status": r.get::<String,_>("status")}))
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id, domain, status FROM domains WHERE id=?")
                .bind(uid.to_string()).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|r| serde_json::json!({"id": r.get::<String,_>("id"), "domain": r.get::<String,_>("domain"), "status": r.get::<String,_>("status")}))
        }
    };
    val.map(|v| Json(serde_json::json!({"success": true, "data": v}))).ok_or(StatusCode::NOT_FOUND)
}

pub async fn remove(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("DELETE FROM domains WHERE id=$1").bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("DELETE FROM domains WHERE id=?").bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn verify(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("UPDATE domains SET status='Active', verified_at=NOW() WHERE id=$1").bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("UPDATE domains SET status='Active', verified_at=? WHERE id=?").bind(Utc::now().to_rfc3339()).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true, "status": "Active"})))
}

pub async fn dns_status(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    // Return DNS check info — in Cloudflare mode fetch from CF API
    if state.config.is_cloudflare() && state.config.cf_api_token.is_some() {
        if let Some(zone) = &state.config.cf_zone_id {
            let client = crate::mail::cloudflare::CfClient::new(state.config.cf_api_token.clone().unwrap());
            if let Ok(v) = client.get_dns_records(zone).await {
                return Ok(Json(serde_json::json!({"success": true, "data": v})));
            }
        }
    }
    Ok(Json(serde_json::json!({"success": true, "data": {"message": "DNS check in VPS mode — configure MX/SPF/DKIM manually", "domain_id": id}})))
}
