use std::sync::Arc;
use axum::{extract::{State, Path}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use chrono::Utc;
use sqlx::Row;
use crate::api::AppState;
use crate::mail::{dkim, dns_check};
use aivory_mail_storage::db::DbPool;
use aivory_mail_core::{validation, dns::{expected_records, DnsRecordInput}};

fn gen_verification_token() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, domain, status, sending_subdomain, created_at FROM domains ORDER BY created_at DESC")
                .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| {
                let id: Uuid = row.get("id");
                serde_json::json!({"id": id, "domain": row.get::<String,_>("domain"), "status": row.get::<String,_>("status"), "sending_subdomain": row.get::<Option<String>,_>("sending_subdomain"), "created_at": row.try_get::<chrono::DateTime<Utc>,_>("created_at").unwrap_or_else(|_| chrono::DateTime::parse_from_rfc3339(&row.try_get::<String,_>("created_at").unwrap_or_default()).map(|d| d.with_timezone(&chrono::Utc)).unwrap_or(chrono::Utc::now()))})
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
    let token = gen_verification_token();
    let (dkim_priv, dkim_pub) = dkim::generate_keypair().map_err(|e| { tracing::error!("dkim keygen: {}", e); StatusCode::INTERNAL_SERVER_ERROR })?;

    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO domains (id, tenant_id, domain, status, dkim_selector, verification_token, dkim_public_key, dkim_private_key, created_at) VALUES ($1,$2,$3,'Pending','aivory',$4,$5,$6,NOW())")
                .bind(id).bind(tenant_id).bind(&domain).bind(&token).bind(&dkim_pub).bind(&dkim_priv)
                .execute(pool).await.map_err(|e| { tracing::error!("insert domain: {}", e); StatusCode::CONFLICT })?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO domains (id, tenant_id, domain, status, dkim_selector, verification_token, dkim_public_key, dkim_private_key, created_at) VALUES (?,?,?,?,?,?,?,?,?)")
                .bind(id.to_string()).bind(tenant_id.to_string()).bind(&domain).bind("Pending").bind("aivory")
                .bind(&token).bind(&dkim_pub).bind(&dkim_priv).bind(Utc::now().to_rfc3339())
                .execute(pool).await.map_err(|_| StatusCode::CONFLICT)?;
        }
    }

    // Cloudflare auto-provisioning stays as an optional bonus path, never required.
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
            let row = sqlx::query("SELECT id, domain, status, dkim_selector, sending_subdomain, verified_at, failure_reason, created_at FROM domains WHERE id=$1")
                .bind(uid).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|r| serde_json::json!({
                "id": r.get::<Uuid,_>("id"), "domain": r.get::<String,_>("domain"), "status": r.get::<String,_>("status"),
                "dkim_selector": r.get::<String,_>("dkim_selector"), "failure_reason": r.get::<Option<String>,_>("failure_reason"),
            }))
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id, domain, status, dkim_selector, failure_reason FROM domains WHERE id=?")
                .bind(uid.to_string()).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|r| serde_json::json!({
                "id": r.get::<String,_>("id"), "domain": r.get::<String,_>("domain"), "status": r.get::<String,_>("status"),
                "dkim_selector": r.get::<String,_>("dkim_selector"), "failure_reason": r.get::<Option<String>,_>("failure_reason"),
            }))
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

struct DomainRow {
    domain: String,
    dkim_selector: String,
    dkim_public_key: Option<String>,
    verification_token: Option<String>,
}

async fn fetch_domain_row(state: &Arc<AppState>, uid: &Uuid) -> Result<Option<DomainRow>, StatusCode> {
    match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT domain, dkim_selector, dkim_public_key, verification_token FROM domains WHERE id=$1")
                .bind(uid).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(row.map(|r| DomainRow {
                domain: r.get("domain"), dkim_selector: r.get("dkim_selector"),
                dkim_public_key: r.get("dkim_public_key"), verification_token: r.get("verification_token"),
            }))
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT domain, dkim_selector, dkim_public_key, verification_token FROM domains WHERE id=?")
                .bind(uid.to_string()).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(row.map(|r| DomainRow {
                domain: r.get("domain"), dkim_selector: r.get("dkim_selector"),
                dkim_public_key: r.get("dkim_public_key"), verification_token: r.get("verification_token"),
            }))
        }
    }
}

fn build_records(state: &Arc<AppState>, row: &DomainRow) -> Vec<aivory_mail_core::dns::DnsRecord> {
    let input = DnsRecordInput {
        domain: &row.domain,
        dkim_selector: &row.dkim_selector,
        dkim_public_key_b64: row.dkim_public_key.as_deref().unwrap_or(""),
        verification_token: row.verification_token.as_deref().unwrap_or(""),
        mx_host: &state.config.mail_mx_host,
        spf_include_host: &state.config.spf_include_host,
        dmarc_report_address: &state.config.dmarc_report_address,
    };
    expected_records(&input)
}

pub async fn verify(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let row = fetch_domain_row(&state, &uid).await?.ok_or(StatusCode::NOT_FOUND)?;
    let token = row.verification_token.clone().unwrap_or_default();
    let expected_value = format!("aivory-site-verification={}", token);
    let ok = dns_check::verify_ownership(&row.domain, &expected_value).await;

    if ok {
        match &state.db {
            DbPool::Postgres(pool) => { sqlx::query("UPDATE domains SET status='Active', verified_at=NOW(), failure_reason=NULL WHERE id=$1").bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            DbPool::Sqlite(pool) => { sqlx::query("UPDATE domains SET status='Active', verified_at=?, failure_reason=NULL WHERE id=?").bind(Utc::now().to_rfc3339()).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        }
        Ok(Json(serde_json::json!({"success": true, "status": "Active"})))
    } else {
        let reason = format!("TXT record _aivory-verify.{} not found or doesn't match yet — DNS changes can take a few minutes to a few hours to propagate", row.domain);
        match &state.db {
            DbPool::Postgres(pool) => { sqlx::query("UPDATE domains SET failure_reason=$1 WHERE id=$2").bind(&reason).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            DbPool::Sqlite(pool) => { sqlx::query("UPDATE domains SET failure_reason=? WHERE id=?").bind(&reason).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        }
        Ok(Json(serde_json::json!({"success": false, "status": "Pending", "error": reason})))
    }
}

pub async fn dns_status(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let row = fetch_domain_row(&state, &uid).await?.ok_or(StatusCode::NOT_FOUND)?;
    let expected = build_records(&state, &row);
    let checked = dns_check::check_records(expected).await;
    Ok(Json(serde_json::json!({"success": true, "data": {"domain": row.domain, "records": checked}})))
}

/// Public-key-only DKIM record for copy-paste — never exposes the private key.
pub async fn dkim_record(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let row = fetch_domain_row(&state, &uid).await?.ok_or(StatusCode::NOT_FOUND)?;
    let records = build_records(&state, &row);
    let dkim = records.into_iter().find(|r| r.purpose == "dkim");
    Ok(Json(serde_json::json!({"success": true, "data": dkim})))
}
