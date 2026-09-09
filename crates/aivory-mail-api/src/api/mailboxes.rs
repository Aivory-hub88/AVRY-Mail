use crate::{
    api::{audit, authz, AppState},
    imap_password_vault,
};
use aivory_mail_core::validation;
use aivory_mail_storage::db::DbPool;
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    Json,
};
use chrono::Utc;
use serde_json::Value;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, StatusCode> {
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
            r.into_iter()
                .map(|row| {
                    serde_json::json!({
                        "id": row.get::<String,_>("id"),
                        "address": row.get::<String,_>("address"),
                        "display_name": row.get::<Option<String>,_>("display_name"),
                        "is_catch_all": row.get::<i32,_>("is_catch_all") != 0,
                        "domain_id": row.get::<String,_>("domain_id"),
                    })
                })
                .collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let actor = authz::require_admin(&state, &headers).await?;
    let address = body
        .get("address")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    validation::validate_email(address).map_err(|_| StatusCode::BAD_REQUEST)?;
    let norm = validation::normalize_email(address);
    let domain_part = validation::extract_domain(&norm).ok_or(StatusCode::BAD_REQUEST)?;
    let display_name = body
        .get("display_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let is_catch_all = body
        .get("is_catch_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let forward_to = body
        .get("forward_to")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let password = body
        .get("password")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    if let Some(p) = password {
        if p.len() < 8 {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"success": false, "error": "Password must be at least 8 characters"}),
                ),
            ));
        }
    }

    // A password-backed mailbox receives a distinct IMAP/SMTP app password.
    // The Dovecot hash remains one-way; the separate vault value is available
    // only to an authenticated administrator through the reveal endpoint.
    let requested_imap_password = body
        .get("imap_password")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    if let Some(p) = requested_imap_password {
        if p.len() < 8 {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"success": false, "error": "IMAP password must be at least 8 characters"}),
                ),
            ));
        }
    }
    let imap_password = requested_imap_password
        .map(str::to_owned)
        .or_else(|| password.map(|_| Uuid::new_v4().simple().to_string()));
    let password_hash = password.map(aivory_mail_core::password::hash_password);
    let password_hash_dovecot = imap_password
        .as_deref()
        .map(aivory_mail_core::password::hash_dovecot);
    let id = Uuid::new_v4();
    let imap_password_encrypted = imap_password
        .as_deref()
        .map(|credential| {
            imap_password_vault::encrypt(&state.config.imap_password_encryption_key, id, credential)
        })
        .transpose()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let domain_id: Uuid = match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT id FROM domains WHERE lower(domain)=lower($1) LIMIT 1")
                .bind(&domain_part)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|r| r.get::<Uuid, _>("id"))
                .ok_or(StatusCode::BAD_REQUEST)?
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id FROM domains WHERE lower(domain)=lower(?) LIMIT 1")
                .bind(&domain_part)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let s: String = row.ok_or(StatusCode::BAD_REQUEST)?.get("id");
            Uuid::parse_str(&s).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        }
    };

    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO mailboxes (id, tenant_id, domain_id, address, display_name, is_catch_all, forward_to, password_hash, password_hash_dovecot, imap_password_encrypted, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,NOW())")
                .bind(id).bind(Uuid::nil()).bind(domain_id).bind(&norm).bind(&display_name).bind(is_catch_all).bind(&forward_to).bind(&password_hash).bind(&password_hash_dovecot).bind(&imap_password_encrypted)
                .execute(pool).await.map_err(|e| { tracing::error!("insert mailbox: {}", e); StatusCode::CONFLICT })?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO mailboxes (id, tenant_id, domain_id, address, display_name, is_catch_all, forward_to, password_hash, password_hash_dovecot, imap_password_encrypted, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id.to_string()).bind(Uuid::nil().to_string()).bind(domain_id.to_string()).bind(&norm).bind(&display_name).bind(if is_catch_all {1}else{0}).bind(&forward_to).bind(&password_hash).bind(&password_hash_dovecot).bind(&imap_password_encrypted).bind(Utc::now().to_rfc3339())
                .execute(pool).await.map_err(|_| StatusCode::CONFLICT)?;
        }
    }

    if state.config.is_cloudflare()
        && state.config.cf_api_token.is_some()
        && state.config.cf_zone_id.is_some()
    {
        let client =
            crate::mail::cloudflare::CfClient::new(state.config.cf_api_token.clone().unwrap());
        let zone = state.config.cf_zone_id.clone().unwrap();
        let worker = std::env::var("CF_EMAIL_WORKER_NAME").unwrap_or_else(|_| "aivory-mail".into());
        let _ = client.create_routing_rule(&zone, &norm, &worker).await;
    }

    if imap_password.is_some() {
        audit::log(
            &state.db,
            "imap_password.vaulted",
            Some(&actor),
            Some(&id.to_string()),
            Some(&id.to_string()),
            None,
            Some(serde_json::json!({"source": "mailbox_create", "vault_version": "v1"})),
        )
        .await;
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "success": true,
            "data": {"id": id, "address": norm, "imap_password": imap_password}
        })),
    ))
}

pub async fn get_one(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let val: Option<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT id, address, display_name FROM mailboxes WHERE id=$1")
                .bind(uid)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|r| serde_json::json!({"id": r.get::<Uuid,_>("id").to_string(), "address": r.get::<String,_>("address")}))
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id, address FROM mailboxes WHERE id=?")
                .bind(uid.to_string())
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|r| serde_json::json!({"id": r.get::<String,_>("id"), "address": r.get::<String,_>("address")}))
        }
    };
    val.map(|v| Json(serde_json::json!({"success": true, "data": v})))
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    if let Some(name) = body.get("display_name").and_then(|v| v.as_str()) {
        match &state.db {
            DbPool::Postgres(pool) => {
                sqlx::query("UPDATE mailboxes SET display_name=$1 WHERE id=$2")
                    .bind(name)
                    .bind(uid)
                    .execute(pool)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            DbPool::Sqlite(pool) => {
                sqlx::query("UPDATE mailboxes SET display_name=? WHERE id=?")
                    .bind(name)
                    .bind(uid.to_string())
                    .execute(pool)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
        }
    }
    if let Some(pw) = body
        .get("password")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        if pw.len() < 8 {
            return Err(StatusCode::BAD_REQUEST);
        }
        let hash = aivory_mail_core::password::hash_password(pw);
        match &state.db {
            DbPool::Postgres(pool) => {
                sqlx::query("UPDATE mailboxes SET password_hash=$1 WHERE id=$2")
                    .bind(&hash)
                    .bind(uid)
                    .execute(pool)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            DbPool::Sqlite(pool) => {
                sqlx::query("UPDATE mailboxes SET password_hash=? WHERE id=?")
                    .bind(&hash)
                    .bind(uid.to_string())
                    .execute(pool)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

/// Issue or rotate a mail-client credential without changing the web-login
/// password. The Dovecot hash and encrypted vault value are written together.
pub async fn set_imap_password(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let actor = authz::require_admin(&state, &headers).await?;
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let password: String = match body
        .get("password")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        Some(p) => {
            if p.len() < 8 {
                return Err(StatusCode::BAD_REQUEST);
            }
            p.to_string()
        }
        None => Uuid::new_v4().simple().to_string()[..20].to_string(),
    };
    let hash = aivory_mail_core::password::hash_dovecot(&password);
    let encrypted =
        imap_password_vault::encrypt(&state.config.imap_password_encryption_key, uid, &password)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let updated: bool = match &state.db {
        DbPool::Postgres(pool) => sqlx::query(
            "UPDATE mailboxes SET password_hash_dovecot=$1, imap_password_encrypted=$2 WHERE id=$3",
        )
        .bind(&hash)
        .bind(&encrypted)
        .bind(uid)
        .execute(pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .rows_affected()
            > 0,
        DbPool::Sqlite(pool) => sqlx::query(
            "UPDATE mailboxes SET password_hash_dovecot=?, imap_password_encrypted=? WHERE id=?",
        )
        .bind(&hash)
        .bind(&encrypted)
        .bind(uid.to_string())
        .execute(pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .rows_affected()
            > 0,
    };
    if !updated {
        return Err(StatusCode::NOT_FOUND);
    }
    audit::log(
        &state.db,
        "imap_password.rotated",
        Some(&actor),
        Some(&id),
        Some(&id),
        None,
        Some(serde_json::json!({"source": "admin", "vault_version": "v1"})),
    )
    .await;
    Ok(Json(
        serde_json::json!({"success": true, "data": {"password": password}}),
    ))
}

/// Reveal an AES-256-GCM protected IMAP credential to an authenticated admin.
/// This response is intentionally non-cacheable and each attempt is audited
/// without persisting or logging any secret material.
pub async fn reveal_imap_password(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<(HeaderMap, Json<Value>), StatusCode> {
    let actor = authz::require_admin(&state, &headers).await?;
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let mailbox: Option<(String, Option<String>)> = match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("SELECT address, imap_password_encrypted FROM mailboxes WHERE id=$1")
                .bind(uid)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|row| (row.get("address"), row.get("imap_password_encrypted")))
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("SELECT address, imap_password_encrypted FROM mailboxes WHERE id=?")
                .bind(uid.to_string())
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|row| (row.get("address"), row.get("imap_password_encrypted")))
        }
    };
    let (address, encrypted) = mailbox.ok_or(StatusCode::NOT_FOUND)?;
    let encrypted = match encrypted {
        Some(value) if !value.is_empty() => value,
        _ => {
            audit::log(
                &state.db,
                "imap_password.reveal_failed",
                Some(&actor),
                Some(&id),
                Some(&id),
                None,
                Some(serde_json::json!({"reason": "not_vaulted"})),
            )
            .await;
            return Err(StatusCode::CONFLICT);
        }
    };
    let password = match imap_password_vault::decrypt(
        &state.config.imap_password_encryption_key,
        uid,
        &encrypted,
    ) {
        Ok(value) => value,
        Err(_) => {
            audit::log(
                &state.db,
                "imap_password.reveal_failed",
                Some(&actor),
                Some(&id),
                Some(&id),
                None,
                Some(serde_json::json!({"reason": "decrypt_failed"})),
            )
            .await;
            return Err(StatusCode::CONFLICT);
        }
    };
    audit::log(
        &state.db,
        "imap_password.revealed",
        Some(&actor),
        Some(&id),
        Some(&id),
        None,
        Some(serde_json::json!({"vault_version": "v1"})),
    )
    .await;
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    Ok((
        response_headers,
        Json(
            serde_json::json!({"success": true, "data": {"address": address, "password": password}}),
        ),
    ))
}

/// Revoke mail-client access without affecting the web-login password.
pub async fn revoke_imap_password(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    let actor = authz::require_admin(&state, &headers).await?;
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let updated = match &state.db {
        DbPool::Postgres(pool) => sqlx::query("UPDATE mailboxes SET password_hash_dovecot=NULL, imap_password_encrypted=NULL WHERE id=$1").bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.rows_affected(),
        DbPool::Sqlite(pool) => sqlx::query("UPDATE mailboxes SET password_hash_dovecot=NULL, imap_password_encrypted=NULL WHERE id=?").bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.rows_affected(),
    };
    if updated == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    audit::log(
        &state.db,
        "imap_password.revoked",
        Some(&actor),
        Some(&id),
        Some(&id),
        None,
        Some(serde_json::json!({"source": "admin"})),
    )
    .await;
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn remove(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("DELETE FROM mailboxes WHERE id=$1")
                .bind(uid)
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("DELETE FROM mailboxes WHERE id=?")
                .bind(uid.to_string())
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

/// User-scoped mailbox discovery for the webmail/calendar/settings clients.
/// The admin `/v1/mailboxes` endpoint remains instance-wide and separately
/// guarded; this endpoint can only ever return the JWT subject's mailbox.
pub async fn self_list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    let email = authz::authenticated_email(&state, &headers)?;
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let rows = sqlx::query("SELECT id, address, display_name, is_catch_all, domain_id FROM mailboxes WHERE lower(address)=$1 ORDER BY address")
                .bind(email)
                .fetch_all(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            rows.into_iter().map(|row| serde_json::json!({
                "id": row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("id").unwrap_or_default()),
                "address": row.get::<String,_>("address"),
                "display_name": row.get::<Option<String>,_>("display_name"),
                "is_catch_all": row.try_get::<bool,_>("is_catch_all").unwrap_or_else(|_| row.try_get::<i32,_>("is_catch_all").map(|i| i != 0).unwrap_or(false)),
                "domain_id": row.try_get::<Uuid,_>("domain_id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("domain_id").unwrap_or_default()),
            })).collect()
        }
        DbPool::Sqlite(pool) => {
            let rows = sqlx::query("SELECT id, address, display_name, is_catch_all, domain_id FROM mailboxes WHERE lower(address)=? ORDER BY address")
                .bind(email)
                .fetch_all(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            rows.into_iter()
                .map(|row| {
                    serde_json::json!({
                        "id": row.get::<String,_>("id"),
                        "address": row.get::<String,_>("address"),
                        "display_name": row.get::<Option<String>,_>("display_name"),
                        "is_catch_all": row.get::<i32,_>("is_catch_all") != 0,
                        "domain_id": row.get::<String,_>("domain_id"),
                    })
                })
                .collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}
