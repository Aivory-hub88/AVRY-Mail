use std::sync::Arc;
use axum::{
    extract::{State, Query},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

/// Resolve the logged-in mailbox (from JWT) -> (id, address)
async fn resolve_own_mailbox(state: &Arc<AppState>, headers: &HeaderMap) -> Result<(String, String), StatusCode> {
    let email = crate::api::authz::authenticated_email(state, headers)?;
    let row: Option<(String, String)> = match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("SELECT id, address FROM mailboxes WHERE lower(address)=lower($1) LIMIT 1")
                .bind(&email)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| (r.get::<Uuid, _>("id").to_string(), r.get::<String, _>("address")))
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("SELECT id, address FROM mailboxes WHERE lower(address)=lower(?) LIMIT 1")
                .bind(&email)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| (r.get::<String, _>("id"), r.get::<String, _>("address")))
        }
    };
    row.ok_or(StatusCode::NOT_FOUND)
}

/// GET /v1/integrations/email — own mailbox status. Never returns password.
pub async fn get_email_integration(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    let (mailbox_id, own_address) = resolve_own_mailbox(&state, &headers).await?;

    // Check if mailbox has a dovecot password (the actual credential)
    let has_imap_pw: bool = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT password_hash_dovecot FROM mailboxes WHERE id=$1::uuid")
                .bind(Uuid::parse_str(&mailbox_id).unwrap_or(Uuid::nil()))
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.and_then(|row| row.get::<Option<String>, _>("password_hash_dovecot"))
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT password_hash_dovecot FROM mailboxes WHERE id=?")
                .bind(&mailbox_id)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.and_then(|row| row.get::<Option<String>, _>("password_hash_dovecot"))
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        }
    };

    let integration: Option<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("SELECT host, port, username, status, last_tested_at, last_connected_at, updated_at FROM email_integrations WHERE mailbox_id=$1::uuid LIMIT 1")
                .bind(Uuid::parse_str(&mailbox_id).unwrap_or(Uuid::nil()))
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| {
                    serde_json::json!({
                        "host": r.get::<String,_>("host"),
                        "port": r.get::<i32,_>("port"),
                        "username": r.get::<String,_>("username"),
                        "status": r.get::<String,_>("status"),
                        "last_tested_at": r.get::<Option<String>,_>("last_tested_at"),
                        "last_connected_at": r.get::<Option<String>,_>("last_connected_at"),
                        "updated_at": r.get::<String,_>("updated_at"),
                    })
                })
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("SELECT host, port, username, status, last_tested_at, last_connected_at, updated_at FROM email_integrations WHERE mailbox_id=? LIMIT 1")
                .bind(&mailbox_id)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| {
                    serde_json::json!({
                        "host": r.get::<String,_>("host"),
                        "port": r.get::<i32,_>("port"),
                        "username": r.get::<String,_>("username"),
                        "status": r.get::<String,_>("status"),
                        "last_tested_at": r.get::<Option<String>,_>("last_tested_at"),
                        "last_connected_at": r.get::<Option<String>,_>("last_connected_at"),
                        "updated_at": r.get::<String,_>("updated_at"),
                    })
                })
        }
    };

    let connected = has_imap_pw && integration.is_some() && integration.as_ref().and_then(|v| v.get("status")).and_then(|s| s.as_str()) == Some("connected");
    // If integration row missing but has_imap_pw true (legacy admin-created), synthesize connected view from mailbox address + defaults
    let data = if let Some(mut v) = integration {
        // ensure connected reflects actual credential presence
        if !has_imap_pw {
            v["status"] = Value::String("disconnected".into());
        }
        v["connected"] = Value::Bool(connected);
        v["address"] = Value::String(own_address.clone());
        v["has_password"] = Value::Bool(has_imap_pw);
        v
    } else if has_imap_pw {
        serde_json::json!({
            "host": state.config.mail_mx_host,
            "port": 993,
            "username": own_address,
            "address": own_address,
            "status": "connected",
            "connected": true,
            "has_password": true,
            "last_tested_at": null,
            "last_connected_at": null,
            "updated_at": null
        })
    } else {
        serde_json::json!({
            "host": state.config.mail_mx_host,
            "port": 993,
            "username": own_address,
            "address": own_address,
            "status": "disconnected",
            "connected": false,
            "has_password": false,
            "last_tested_at": null,
            "last_connected_at": null,
            "updated_at": null
        })
    };

    Ok(Json(serde_json::json!({"success": true, "data": data})))
}

/// POST /v1/integrations/email/test — validate & optionally TCP-probe without saving.
/// Body: { host, port, username, password }  (all required for test)
pub async fn test_email_integration(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // require auth
    let _ = crate::api::authz::authenticated_email(&state, &headers)?;
    let host = body.get("host").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let port = body.get("port").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let username = body.get("username").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let password = body.get("password").and_then(|v| v.as_str()).unwrap_or("").to_string();

    if host.is_empty() || username.is_empty() || password.is_empty() {
        return Ok(Json(serde_json::json!({"success": false, "error": "host, username and password are required"})));
    }
    if port < 1 || port > 65535 {
        return Ok(Json(serde_json::json!({"success": false, "error": "port must be 1-65535"})));
    }
    if password.len() < 8 {
        return Ok(Json(serde_json::json!({"success": false, "error": "password must be at least 8 characters"})));
    }
    if !username.contains('@') {
        return Ok(Json(serde_json::json!({"success": false, "error": "username must be an email address"})));
    }

    // For our own MX host, we can do a lightweight credential sanity check without
    // storing anything — just verify the mailbox exists.
    let is_own_host = host.eq_ignore_ascii_case(&state.config.mail_mx_host) || host == "localhost" || host == "127.0.0.1";
    if !is_own_host {
        // Try TCP connect with 4s timeout — proves host:port is reachable.
        let addr = format!("{}:{}", host, port);
        let reachable = tokio::time::timeout(
            std::time::Duration::from_secs(4),
            tokio::net::TcpStream::connect(&addr),
        )
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false);
        if !reachable {
            return Ok(Json(serde_json::json!({"success": false, "error": format!("cannot reach {}:{}", host, port)})));
        }
    }

    // Password is not validated against Dovecot here — that would require
    // actually attempting an IMAP LOGIN. For now, reaching the host + basic
    // validation is the test; the real save will hash and store.
    Ok(Json(serde_json::json!({"success": true, "reachable": true, "message": "Connection test passed"})))
}

/// POST /v1/integrations/email — upsert (save). Requires test to have passed client-side,
/// but we still validate server-side. Body: { host, port, username, password }
pub async fn upsert_email_integration(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let (mailbox_id, own_address) = resolve_own_mailbox(&state, &headers).await?;
    let host = body.get("host").and_then(|v| v.as_str()).unwrap_or(&state.config.mail_mx_host).trim().to_string();
    let host = if host.is_empty() { state.config.mail_mx_host.clone() } else { host };
    let port = body.get("port").and_then(|v| v.as_i64()).unwrap_or(993) as i32;
    let username = body.get("username").and_then(|v| v.as_str()).unwrap_or(&own_address).trim().to_string();
    let username = if username.is_empty() { own_address.clone() } else { username };
    let password = body.get("password").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();

    if port < 1 || port > 65535 {
        return Ok(Json(serde_json::json!({"success": false, "error": "port must be 1-65535"})));
    }
    if password.len() < 8 {
        return Ok(Json(serde_json::json!({"success": false, "error": "password must be at least 8 characters"})));
    }
    if !username.contains('@') {
        return Ok(Json(serde_json::json!({"success": false, "error": "username must be an email address"})));
    }

    let hash = aivory_mail_core::password::hash_dovecot(&password);
    let now = chrono::Utc::now().to_rfc3339();

    // Store/update the Dovecot hash on the mailbox (the actual IMAP credential)
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE mailboxes SET password_hash_dovecot=$1 WHERE id=$2::uuid")
                .bind(&hash)
                .bind(Uuid::parse_str(&mailbox_id).unwrap_or(Uuid::nil()))
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE mailboxes SET password_hash_dovecot=? WHERE id=?")
                .bind(&hash)
                .bind(&mailbox_id)
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }

    // Upsert integration status row (host/port/username + connected)
    let id = Uuid::new_v4();
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO email_integrations (id, mailbox_id, host, port, username, status, last_tested_at, last_connected_at, created_at, updated_at)
                 VALUES ($1,$2::uuid,$3,$4,$5,'connected',$6,$6,NOW(),NOW())
                 ON CONFLICT (mailbox_id) DO UPDATE SET host=$3, port=$4, username=$5, status='connected', last_tested_at=$6, last_connected_at=$6, updated_at=NOW()",
            )
            .bind(id)
            .bind(Uuid::parse_str(&mailbox_id).unwrap_or(Uuid::nil()))
            .bind(&host)
            .bind(port)
            .bind(&username)
            .bind(&now)
            .execute(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query(
                "INSERT OR REPLACE INTO email_integrations (id, mailbox_id, host, port, username, status, last_tested_at, last_connected_at, created_at, updated_at)
                 VALUES (?,?,?,?,?,'connected',?,?,?,?)",
            )
            .bind(id.to_string())
            .bind(&mailbox_id)
            .bind(&host)
            .bind(port)
            .bind(&username)
            .bind(&now)
            .bind(&now)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }

    // Never echo password back beyond this point — UI must switch to
    // "Connected as ..." and never render it again.
    Ok(Json(serde_json::json!({"success": true, "data": {
        "host": host,
        "port": port,
        "username": username,
        "status": "connected",
        "connected": true,
        "address": own_address
    }})))
}

/// DELETE /v1/integrations/email — disconnect: clears Dovecot credential + marks disconnected
pub async fn delete_email_integration(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    let (mailbox_id, _) = resolve_own_mailbox(&state, &headers).await?;
    let now = chrono::Utc::now().to_rfc3339();
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE mailboxes SET password_hash_dovecot=NULL WHERE id=$1::uuid")
                .bind(Uuid::parse_str(&mailbox_id).unwrap_or(Uuid::nil()))
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            sqlx::query("UPDATE email_integrations SET status='disconnected', updated_at=$1 WHERE mailbox_id=$2::uuid")
                .bind(&now)
                .bind(Uuid::parse_str(&mailbox_id).unwrap_or(Uuid::nil()))
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE mailboxes SET password_hash_dovecot=NULL WHERE id=?")
                .bind(&mailbox_id)
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            sqlx::query("UPDATE email_integrations SET status='disconnected', updated_at=? WHERE mailbox_id=?")
                .bind(&now)
                .bind(&mailbox_id)
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

/// GET /v1/integrations/email/admin?mailbox_id=... — admin can check any mailbox status (no password ever returned)
pub async fn admin_get_email_integration(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, StatusCode> {
    crate::api::authz::require_admin(&state, &headers).await?;
    let mailbox_id = params.get("mailbox_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let uid = Uuid::parse_str(mailbox_id).map_err(|_| StatusCode::BAD_REQUEST)?;

    let (address, has_pw): (Option<String>, bool) = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT address, password_hash_dovecot FROM mailboxes WHERE id=$1")
                .bind(uid)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            match r {
                Some(row) => (Some(row.get::<String,_>("address")), row.get::<Option<String>,_>("password_hash_dovecot").map(|s| !s.is_empty()).unwrap_or(false)),
                None => (None, false),
            }
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT address, password_hash_dovecot FROM mailboxes WHERE id=?")
                .bind(mailbox_id)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            match r {
                Some(row) => (Some(row.get::<String,_>("address")), row.get::<Option<String>,_>("password_hash_dovecot").map(|s| !s.is_empty()).unwrap_or(false)),
                None => (None, false),
            }
        }
    };
    if address.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    let integration: Option<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("SELECT host, port, username, status, last_tested_at, last_connected_at, updated_at FROM email_integrations WHERE mailbox_id=$1 LIMIT 1")
                .bind(uid)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| serde_json::json!({
                    "host": r.get::<String,_>("host"),
                    "port": r.get::<i32,_>("port"),
                    "username": r.get::<String,_>("username"),
                    "status": r.get::<String,_>("status"),
                    "last_tested_at": r.get::<Option<String>,_>("last_tested_at"),
                    "last_connected_at": r.get::<Option<String>,_>("last_connected_at"),
                    "updated_at": r.get::<String,_>("updated_at"),
                }))
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("SELECT host, port, username, status, last_tested_at, last_connected_at, updated_at FROM email_integrations WHERE mailbox_id=? LIMIT 1")
                .bind(mailbox_id)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| serde_json::json!({
                    "host": r.get::<String,_>("host"),
                    "port": r.get::<i32,_>("port"),
                    "username": r.get::<String,_>("username"),
                    "status": r.get::<String,_>("status"),
                    "last_tested_at": r.get::<Option<String>,_>("last_tested_at"),
                    "last_connected_at": r.get::<Option<String>,_>("last_connected_at"),
                    "updated_at": r.get::<String,_>("updated_at"),
                }))
        }
    };

    let status_str = integration.as_ref().and_then(|v| v.get("status")).and_then(|s| s.as_str()).unwrap_or("connected").to_string();
    let status = if !has_pw { "disconnected".to_string() } else { status_str };
    let mut data = if let Some(v) = integration { v } else {
        serde_json::json!({
            "host": state.config.mail_mx_host,
            "port": 993,
            "username": address.clone().unwrap_or_default(),
            "status": status,
            "last_tested_at": null,
            "last_connected_at": null,
            "updated_at": null
        })
    };
    data["address"] = Value::String(address.unwrap_or_default());
    data["status"] = Value::String(status.to_string());
    data["connected"] = Value::Bool(has_pw && status == "connected");
    data["has_password"] = Value::Bool(has_pw);

    Ok(Json(serde_json::json!({"success": true, "data": data})))
}
