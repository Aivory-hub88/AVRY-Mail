use crate::api::{execution_context::OwnerType, AppState};
use aivory_mail_storage::db::DbPool;
use axum::http::{HeaderMap, StatusCode};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

pub const MCP_AUDIENCE: &str = "aivory-mail-mcp";
pub const DEFAULT_LIFETIME_SECONDS: i64 = 15 * 60;
pub const MAX_LIFETIME_SECONDS: i64 = 60 * 60;

const ALLOWED_SCOPES: &[&str] = &[
    "mail.read",
    "mail.search",
    "mail.thread.read",
    "mail.knowledge.read",
    "mail.attachment.read",
    "mail.draft.create",
    "mail.send",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityContext {
    pub grant_id: Uuid,
    pub tenant_id: Uuid,
    pub mailbox_id: Uuid,
    pub owner_type: OwnerType,
    pub caller_id: String,
    pub audience: String,
    pub scopes: Vec<String>,
    pub jti: Uuid,
    pub expires_at: DateTime<Utc>,
}

impl CapabilityContext {
    pub fn require_scope(&self, scope: &str) -> Result<(), StatusCode> {
        if self.scopes.iter().any(|candidate| candidate == scope) {
            Ok(())
        } else {
            Err(StatusCode::FORBIDDEN)
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueCapabilityRequest {
    pub tenant_id: String,
    pub mailbox_id: String,
    pub caller_id: String,
    pub audience: String,
    pub scopes: Vec<String>,
    pub expires_in_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityMetadata {
    pub id: String,
    pub tenant_id: String,
    pub mailbox_id: String,
    pub caller_id: String,
    pub audience: String,
    pub scopes: Vec<String>,
    pub jti: String,
    pub expires_at: String,
    pub revoked_at: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
struct StoredCapability {
    metadata: CapabilityMetadata,
    expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("avry_mcp_{}", URL_SAFE_NO_PAD.encode(bytes))
}

fn validate_scopes(scopes: &[String]) -> Result<Vec<String>, StatusCode> {
    if scopes.is_empty() || scopes.len() > ALLOWED_SCOPES.len() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut normalized = Vec::with_capacity(scopes.len());
    for scope in scopes {
        let value = scope.trim().to_lowercase();
        if !ALLOWED_SCOPES.contains(&value.as_str()) || normalized.contains(&value) {
            return Err(StatusCode::BAD_REQUEST);
        }
        normalized.push(value);
    }
    Ok(normalized)
}

fn parse_uuid(value: &str) -> Result<Uuid, StatusCode> {
    Uuid::parse_str(value.trim()).map_err(|_| StatusCode::BAD_REQUEST)
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, StatusCode> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn context_from_stored(
    stored: &StoredCapability,
    expected_audience: &str,
    expected_caller: Option<&str>,
    now: DateTime<Utc>,
) -> Result<CapabilityContext, StatusCode> {
    if stored.revoked_at.is_some() || stored.expires_at <= now {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if stored.metadata.audience != expected_audience {
        return Err(StatusCode::FORBIDDEN);
    }
    if expected_caller.is_some_and(|caller| caller != stored.metadata.caller_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    let tenant_id =
        parse_uuid(&stored.metadata.tenant_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mailbox_id =
        parse_uuid(&stored.metadata.mailbox_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let grant_id =
        parse_uuid(&stored.metadata.id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let jti = parse_uuid(&stored.metadata.jti).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let scopes =
        validate_scopes(&stored.metadata.scopes).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(CapabilityContext {
        grant_id,
        tenant_id,
        mailbox_id,
        owner_type: OwnerType::Agent,
        caller_id: stored.metadata.caller_id.clone(),
        audience: stored.metadata.audience.clone(),
        scopes,
        jti,
        expires_at: stored.expires_at,
    })
}

fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn metadata_from_postgres_row(row: &sqlx::postgres::PgRow) -> Result<StoredCapability, StatusCode> {
    let id: Uuid = row
        .try_get("id")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tenant_id: Uuid = row
        .try_get("tenant_id")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mailbox_id: Uuid = row
        .try_get("mailbox_id")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let jti: Uuid = row
        .try_get("jti")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let caller_id: String = row
        .try_get("caller_id")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let audience: String = row
        .try_get("audience")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let scopes_json: String = row
        .try_get("scopes")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let scopes: Vec<String> =
        serde_json::from_str(&scopes_json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let expires_at: DateTime<Utc> = row
        .try_get("expires_at")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let revoked_at: Option<DateTime<Utc>> = row
        .try_get("revoked_at")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let last_used_at: Option<DateTime<Utc>> = row
        .try_get("last_used_at")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let created_at: DateTime<Utc> = row
        .try_get("created_at")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StoredCapability {
        metadata: CapabilityMetadata {
            id: id.to_string(),
            tenant_id: tenant_id.to_string(),
            mailbox_id: mailbox_id.to_string(),
            caller_id,
            audience,
            scopes,
            jti: jti.to_string(),
            expires_at: expires_at.to_rfc3339(),
            revoked_at: revoked_at.map(|value| value.to_rfc3339()),
            last_used_at: last_used_at.map(|value| value.to_rfc3339()),
            created_at: created_at.to_rfc3339(),
        },
        expires_at,
        revoked_at,
    })
}

fn metadata_from_sqlite_row(row: &sqlx::sqlite::SqliteRow) -> Result<StoredCapability, StatusCode> {
    let id = parse_uuid(&row.get::<String, _>("id"))?;
    let tenant_id = parse_uuid(&row.get::<String, _>("tenant_id"))?;
    let mailbox_id = parse_uuid(&row.get::<String, _>("mailbox_id"))?;
    let jti = parse_uuid(&row.get::<String, _>("jti"))?;
    let caller_id: String = row.get("caller_id");
    let audience: String = row.get("audience");
    let scopes_json: String = row.get("scopes");
    let scopes: Vec<String> =
        serde_json::from_str(&scopes_json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let expires_at = parse_timestamp(&row.get::<String, _>("expires_at"))?;
    let revoked_at = row
        .get::<Option<String>, _>("revoked_at")
        .map(|value| parse_timestamp(&value))
        .transpose()?;
    let last_used_at = row
        .get::<Option<String>, _>("last_used_at")
        .map(|value| parse_timestamp(&value))
        .transpose()?;
    let created_at = parse_timestamp(&row.get::<String, _>("created_at"))?;

    Ok(StoredCapability {
        metadata: CapabilityMetadata {
            id: id.to_string(),
            tenant_id: tenant_id.to_string(),
            mailbox_id: mailbox_id.to_string(),
            caller_id,
            audience,
            scopes,
            jti: jti.to_string(),
            expires_at: expires_at.to_rfc3339(),
            revoked_at: revoked_at.map(|value| value.to_rfc3339()),
            last_used_at: last_used_at.map(|value| value.to_rfc3339()),
            created_at: created_at.to_rfc3339(),
        },
        expires_at,
        revoked_at,
    })
}

pub async fn issue_capability(
    state: &Arc<AppState>,
    request: IssueCapabilityRequest,
) -> Result<(CapabilityMetadata, String), StatusCode> {
    let tenant_id = parse_uuid(&request.tenant_id)?;
    let mailbox_id = parse_uuid(&request.mailbox_id)?;
    let caller_id = request.caller_id.trim().to_string();
    let audience = request.audience.trim().to_string();
    if caller_id.is_empty() || caller_id.len() > 200 || audience != MCP_AUDIENCE {
        return Err(StatusCode::BAD_REQUEST);
    }
    let scopes = validate_scopes(&request.scopes)?;
    let lifetime = request
        .expires_in_seconds
        .unwrap_or(DEFAULT_LIFETIME_SECONDS);
    if !(1..=MAX_LIFETIME_SECONDS).contains(&lifetime) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let expires_at = Utc::now() + Duration::seconds(lifetime);
    let id = Uuid::new_v4();
    let jti = Uuid::new_v4();
    let raw_token = generate_token();
    let token_hash = hash_token(&raw_token);
    let scopes_json =
        serde_json::to_string(&scopes).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match &state.db {
        DbPool::Postgres(pool) => {
            let mailbox_tenant: Option<Uuid> =
                sqlx::query_scalar("SELECT tenant_id FROM mailboxes WHERE id=$1")
                    .bind(mailbox_id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if mailbox_tenant != Some(tenant_id) {
                return Err(StatusCode::FORBIDDEN);
            }
            sqlx::query("INSERT INTO mcp_capability_grants (id, tenant_id, mailbox_id, caller_id, audience, scopes, token_hash, jti, expires_at, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
                .bind(id)
                .bind(tenant_id)
                .bind(mailbox_id)
                .bind(&caller_id)
                .bind(&audience)
                .bind(&scopes_json)
                .bind(&token_hash)
                .bind(jti)
                .bind(expires_at)
                .bind(Utc::now())
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            let mailbox_tenant: Option<String> =
                sqlx::query_scalar("SELECT tenant_id FROM mailboxes WHERE id=?")
                    .bind(mailbox_id.to_string())
                    .fetch_optional(pool)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if mailbox_tenant.as_deref() != Some(&tenant_id.to_string()) {
                return Err(StatusCode::FORBIDDEN);
            }
            sqlx::query("INSERT INTO mcp_capability_grants (id, tenant_id, mailbox_id, caller_id, audience, scopes, token_hash, jti, expires_at, created_at) VALUES (?,?,?,?,?,?,?,?,?,?)")
                .bind(id.to_string())
                .bind(tenant_id.to_string())
                .bind(mailbox_id.to_string())
                .bind(&caller_id)
                .bind(&audience)
                .bind(&scopes_json)
                .bind(&token_hash)
                .bind(jti.to_string())
                .bind(expires_at.to_rfc3339())
                .bind(Utc::now().to_rfc3339())
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }

    Ok((
        CapabilityMetadata {
            id: id.to_string(),
            tenant_id: tenant_id.to_string(),
            mailbox_id: mailbox_id.to_string(),
            caller_id,
            audience,
            scopes,
            jti: jti.to_string(),
            expires_at: expires_at.to_rfc3339(),
            revoked_at: None,
            last_used_at: None,
            created_at: Utc::now().to_rfc3339(),
        },
        raw_token,
    ))
}

async fn validate_mailbox_pair(
    state: &Arc<AppState>,
    tenant_id: Uuid,
    mailbox_id: Uuid,
) -> Result<(), StatusCode> {
    let valid = match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query_scalar::<_, Uuid>("SELECT tenant_id FROM mailboxes WHERE id=$1")
                .bind(mailbox_id)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .is_some_and(|current| current == tenant_id)
        }
        DbPool::Sqlite(pool) => {
            let current = sqlx::query_scalar::<_, String>(
                "SELECT tenant_id FROM mailboxes WHERE id=?",
            )
            .bind(mailbox_id.to_string())
            .fetch_optional(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            current.as_deref() == Some(tenant_id.to_string().as_str())
        }
    };
    if valid {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

pub async fn validate_capability(
    state: &Arc<AppState>,
    raw_token: &str,
    expected_audience: &str,
    expected_caller: Option<&str>,
) -> Result<CapabilityContext, StatusCode> {
    if raw_token.trim().is_empty() || raw_token.len() > 256 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let token_hash = hash_token(raw_token.trim());
    let stored = match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT id, tenant_id, mailbox_id, caller_id, audience, scopes, jti, expires_at, revoked_at, last_used_at, created_at FROM mcp_capability_grants WHERE token_hash=$1 LIMIT 1")
                .bind(&token_hash)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|row| metadata_from_postgres_row(&row))
                .transpose()?
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id, tenant_id, mailbox_id, caller_id, audience, scopes, jti, expires_at, revoked_at, last_used_at, created_at FROM mcp_capability_grants WHERE token_hash=? LIMIT 1")
                .bind(&token_hash)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            row.map(|row| metadata_from_sqlite_row(&row)).transpose()?
        }
    };
    let stored = stored.ok_or(StatusCode::UNAUTHORIZED)?;
    let context = context_from_stored(&stored, expected_audience, expected_caller, Utc::now())?;
    validate_mailbox_pair(state, context.tenant_id, context.mailbox_id).await?;

    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE mcp_capability_grants SET last_used_at=NOW() WHERE id=$1 AND revoked_at IS NULL")
                .bind(context.grant_id)
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query(
                "UPDATE mcp_capability_grants SET last_used_at=? WHERE id=? AND revoked_at IS NULL",
            )
            .bind(Utc::now().to_rfc3339())
            .bind(context.grant_id.to_string())
            .execute(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }

    Ok(context)
}

pub async fn resolve_mcp_capability(
    state: &Arc<AppState>,
    headers: &HeaderMap,
) -> Result<CapabilityContext, StatusCode> {
    let raw = extract_bearer(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let expected_caller = headers
        .get("x-agent-id")
        .and_then(|value| value.to_str().ok());
    validate_capability(state, raw, MCP_AUDIENCE, expected_caller).await
}

pub async fn revoke_capability(state: &Arc<AppState>, grant_id: Uuid) -> Result<(), StatusCode> {
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE mcp_capability_grants SET revoked_at=NOW() WHERE id=$1")
                .bind(grant_id)
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE mcp_capability_grants SET revoked_at=? WHERE id=?")
                .bind(Utc::now().to_rfc3339())
                .bind(grant_id.to_string())
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(())
}

pub async fn list_capabilities(
    state: &Arc<AppState>,
) -> Result<Vec<CapabilityMetadata>, StatusCode> {
    match &state.db {
        DbPool::Postgres(pool) => {
            let rows = sqlx::query("SELECT id, tenant_id, mailbox_id, caller_id, audience, scopes, jti, expires_at, revoked_at, last_used_at, created_at FROM mcp_capability_grants ORDER BY created_at DESC")
                .fetch_all(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            rows.iter()
                .map(|row| metadata_from_postgres_row(row).map(|stored| stored.metadata))
                .collect()
        }
        DbPool::Sqlite(pool) => {
            let rows = sqlx::query("SELECT id, tenant_id, mailbox_id, caller_id, audience, scopes, jti, expires_at, revoked_at, last_used_at, created_at FROM mcp_capability_grants ORDER BY created_at DESC")
                .fetch_all(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            rows.iter()
                .map(|row| metadata_from_sqlite_row(row).map(|stored| stored.metadata))
                .collect()
        }
    }
}

pub fn issue_body_to_value(metadata: &CapabilityMetadata, raw_token: &str) -> Value {
    serde_json::json!({
        "grant": metadata,
        "access_token": raw_token,
        "token_type": "Bearer"
    })
}

#[cfg(test)]
mod tests {
    use super::{
        context_from_stored, extract_bearer, hash_token, validate_scopes, CapabilityMetadata,
        StoredCapability, ALLOWED_SCOPES,
    };
    use axum::http::{HeaderMap, HeaderValue, StatusCode};
    use chrono::{Duration, Utc};
    use uuid::Uuid;

    fn stored(
        expires_at: chrono::DateTime<Utc>,
        revoked_at: Option<chrono::DateTime<Utc>>,
    ) -> StoredCapability {
        let id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let mailbox_id = Uuid::new_v4();
        let jti = Uuid::new_v4();
        StoredCapability {
            metadata: CapabilityMetadata {
                id: id.to_string(),
                tenant_id: tenant_id.to_string(),
                mailbox_id: mailbox_id.to_string(),
                caller_id: "zeroclaw-mail-assistant".to_string(),
                audience: super::MCP_AUDIENCE.to_string(),
                scopes: vec!["mail.search".to_string()],
                jti: jti.to_string(),
                expires_at: expires_at.to_rfc3339(),
                revoked_at: revoked_at.map(|value| value.to_rfc3339()),
                last_used_at: None,
                created_at: Utc::now().to_rfc3339(),
            },
            expires_at,
            revoked_at,
        }
    }

    #[test]
    fn token_hash_is_one_way_stable_identifier() {
        assert_eq!(hash_token("token"), hash_token("token"));
        assert_ne!(hash_token("token"), hash_token("other"));
    }

    #[test]
    fn scopes_are_explicit_and_default_deny_unknown_values() {
        assert!(validate_scopes(&["mail.search".to_string()]).is_ok());
        assert!(validate_scopes(&["mail.*".to_string()]).is_err());
        assert!(validate_scopes(&[]).is_err());
        assert!(ALLOWED_SCOPES.contains(&"mail.send"));
    }

    #[test]
    fn bearer_parser_rejects_missing_and_non_bearer_credentials() {
        assert!(extract_bearer(&HeaderMap::new()).is_none());
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Basic abc"));
        assert!(extract_bearer(&headers).is_none());
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer capability"),
        );
        assert_eq!(extract_bearer(&headers), Some("capability"));
    }

    #[test]
    fn context_validation_denies_expired_revoked_wrong_audience_and_caller() {
        let now = Utc::now();
        assert_eq!(
            context_from_stored(
                &stored(now - Duration::seconds(1), None),
                super::MCP_AUDIENCE,
                None,
                now
            ),
            Err(StatusCode::UNAUTHORIZED)
        );
        assert_eq!(
            context_from_stored(
                &stored(now + Duration::seconds(60), Some(now)),
                super::MCP_AUDIENCE,
                None,
                now
            ),
            Err(StatusCode::UNAUTHORIZED)
        );
        assert_eq!(
            context_from_stored(
                &stored(now + Duration::seconds(60), None),
                "wrong-audience",
                None,
                now
            ),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            context_from_stored(
                &stored(now + Duration::seconds(60), None),
                super::MCP_AUDIENCE,
                Some("other-agent"),
                now
            ),
            Err(StatusCode::FORBIDDEN)
        );
    }
}
