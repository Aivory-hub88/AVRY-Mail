use std::sync::Arc;
use axum::{extract::{State, Path}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use sqlx::Row;
use sha2::{Sha256, Digest};
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

fn hash_key(raw: &str) -> String {
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    format!("{:x}", h.finalize())
}
fn mask(raw: &str) -> String {
    if raw.len() <= 12 { format!("{}****", &raw[..4.min(raw.len())]) }
    else { format!("{}–{}****{}****", &raw[..8], &raw[8..12], "*".repeat(12)) }
    // Simpler Tavily style: avry-dev-****... keep prefix visible
}
fn gen_raw(name: &str) -> String {
    let prefix = if name.to_lowercase().contains("dev") { "avry-dev-" } else { "avry-" };
    format!("{}{}{}", prefix, Uuid::new_v4().to_string().replace('-', ""), Uuid::new_v4().to_string().replace('-', "")[..8].to_string())
}

pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, tenant_id, name, key_hash, COALESCE(key_raw,'') as key_raw, created_at FROM api_keys ORDER BY created_at DESC").fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| {
                let hash: String = row.get("key_hash");
                let raw: String = row.get("key_raw");
                let masked = if raw.len() > 12 { format!("{}****{}", &raw[..12], &raw[raw.len()-4..]) } else { format!("{}****", &raw[..8.min(raw.len())]) };
                serde_json::json!({"id": row.get::<Uuid,_>("id").to_string(), "name": row.get::<String,_>("name"), "key_masked": masked, "key_hash": hash, "key_raw": raw, "created_at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339()})
            }).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, name, key_hash, COALESCE(key_raw,'') as key_raw, created_at FROM api_keys ORDER BY created_at DESC").fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| {
                let hash: String = row.get("key_hash");
                let raw: String = row.get("key_raw");
                let masked = if raw.len() > 12 { format!("{}****{}", &raw[..12], &raw[raw.len()-4..]) } else { format!("{}****", &raw[..8.min(raw.len())]) };
                serde_json::json!({"id": row.get::<String,_>("id"), "name": row.get::<String,_>("name"), "key_masked": masked, "key_raw": raw, "created_at": row.get::<String,_>("created_at")})
            }).collect()
        }
    };
    if rows.is_empty() {
        // Auto-create default dev key for demo (like Tavily default)
        let raw = gen_raw("default");
        let hash = hash_key(&raw);
        let id = Uuid::new_v4();
        match &state.db {
            DbPool::Postgres(pool) => { let _ = sqlx::query("INSERT INTO api_keys (id, tenant_id, name, key_hash, key_raw, created_at) VALUES ($1,$2,$3,$4,$5,NOW())").bind(id).bind(Uuid::nil()).bind("default").bind(&hash).bind(&raw).execute(pool).await; }
            DbPool::Sqlite(pool) => { let _ = sqlx::query("INSERT INTO api_keys (id, tenant_id, name, key_hash, key_raw, created_at) VALUES (?,?,?,?,?,?)").bind(id.to_string()).bind(Uuid::nil().to_string()).bind("default").bind(&hash).bind(&raw).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await; }
        }
        let masked_auto = if raw.len() > 12 { format!("{}****{}", &raw[..12], &raw[raw.len()-4..]) } else { format!("{}****", &raw[..8.min(raw.len())]) };
        return Ok(Json(serde_json::json!({"success": true, "data": [{"id": id.to_string(), "name": "default", "key_masked": masked_auto, "key_raw": raw, "created_at": chrono::Utc::now().to_rfc3339()}], "hint": "auto-created default dev key — copy now, raw shown once"})));
    }
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn create(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("dev").to_string();
    let raw = gen_raw(&name);
    let hash = hash_key(&raw);
    let id = Uuid::new_v4();
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("INSERT INTO api_keys (id, tenant_id, name, key_hash, key_raw, created_at) VALUES ($1,$2,$3,$4,$5,NOW())").bind(id).bind(Uuid::nil()).bind(&name).bind(&hash).bind(&raw).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("INSERT INTO api_keys (id, tenant_id, name, key_hash, key_raw, created_at) VALUES (?,?,?,?,?,?)").bind(id.to_string()).bind(Uuid::nil().to_string()).bind(&name).bind(&hash).bind(&raw).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({"success": true, "data": {"id": id.to_string(), "name": name, "key_raw": raw, "key_masked": format!("avry-****{}", &hash[..8])}}))))
}

pub async fn remove(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("DELETE FROM api_keys WHERE id=$1").bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("DELETE FROM api_keys WHERE id=?").bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn generate_mcp_link(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let key_id = body.get("key_id").or_else(|| body.get("api_key_id")).and_then(|v| v.as_str());
    let key_name = body.get("name").and_then(|v| v.as_str()).unwrap_or("default");
    // Find key hash
    let hash: Option<String> = if let Some(kid) = key_id {
        let uid = Uuid::parse_str(kid).map_err(|_| StatusCode::BAD_REQUEST)?;
        match &state.db {
            DbPool::Postgres(pool) => {
                let row = sqlx::query("SELECT key_hash FROM api_keys WHERE id=$1").bind(uid).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                row.map(|r| r.get::<String,_>("key_hash"))
            }
            DbPool::Sqlite(pool) => {
                let row = sqlx::query("SELECT key_hash FROM api_keys WHERE id=?").bind(uid.to_string()).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                row.map(|r| r.get::<String,_>("key_hash"))
            }
        }
    } else {
        match &state.db {
            DbPool::Postgres(pool) => {
                let row = sqlx::query("SELECT key_hash FROM api_keys WHERE name=$1 LIMIT 1").bind(key_name).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                row.map(|r| r.get::<String,_>("key_hash"))
            }
            DbPool::Sqlite(pool) => {
                let row = sqlx::query("SELECT key_hash FROM api_keys WHERE name=? LIMIT 1").bind(key_name).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                row.map(|r| r.get::<String,_>("key_hash"))
            }
        }
    };
    let base = std::env::var("PUBLIC_MAIL_URL").or_else(|_| std::env::var("MAIL_PUBLIC_URL")).unwrap_or_else(|_| "https://mail.aivory.uk".into());
    // For Tavily-style, link is like https://mail.aivory.uk/mcp?api_key=xxx or http://avry-mail:8095/mcp
    let mcp_url = format!("{}/mcp", base.trim_end_matches('/'));
    let link = if let Some(h) = hash { format!("{}?api_key=avry-****{}&name={}", mcp_url, &h[..8], key_name) } else { mcp_url.clone() };
    Ok(Json(serde_json::json!({"success": true, "data": {"mcp_url": mcp_url, "mcp_link": link, "transport": "streamable-http", "hint": "Use Authorization: Bearer <api_key> or ?api_key= — copy raw key once from create"}})))
}
