use crate::api::{authz, mcp_capabilities, AppState};
use aivory_mail_storage::db::DbPool;
use axum::http::{HeaderMap, StatusCode};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

/// The immutable authorization context for one isolated data-plane request.
///
/// Phase 1 constructs this context from a first-party user session. Agent
/// capabilities, delegated Cerveau grants, and non-empty capability scopes
/// are intentionally deferred to Phase 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionContext {
    pub tenant_id: Uuid,
    pub mailbox_id: Uuid,
    pub principal: String,
    pub owner_type: OwnerType,
    pub caller_id: String,
    pub audience: String,
    pub scopes: Vec<String>,
    pub grant_id: Option<Uuid>,
    pub request_id: String,
    pub trace_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerType {
    User,
    Agent,
}

impl ExecutionContext {
    pub fn require_scope(&self, scope: &str) -> Result<(), StatusCode> {
        if self.scopes.iter().any(|candidate| candidate == scope) {
            Ok(())
        } else {
            Err(StatusCode::FORBIDDEN)
        }
    }

    pub fn from_capability(
        capability: &mcp_capabilities::CapabilityContext,
        headers: &HeaderMap,
    ) -> Self {
        Self {
            tenant_id: capability.tenant_id,
            mailbox_id: capability.mailbox_id,
            principal: capability.caller_id.clone(),
            owner_type: capability.owner_type,
            caller_id: capability.caller_id.clone(),
            audience: capability.audience.clone(),
            scopes: capability.scopes.clone(),
            grant_id: Some(capability.grant_id),
            request_id: headers
                .get("x-request-id")
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            trace_id: headers
                .get("x-trace-id")
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned),
        }
    }
}

/// Resolve exactly one mailbox and its tenant from the authenticated user.
///
/// `requested_mailbox` is a consistency check only. It can never select a
/// mailbox for the caller. Ambiguous or missing mailbox ownership fails closed.
pub async fn resolve_user_context(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    requested_mailbox: Option<&str>,
) -> Result<ExecutionContext, StatusCode> {
    let claims = authz::authenticated_claims(state, headers)?;
    let principal = claims.sub.trim().to_lowercase();
    if principal.is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let requested = requested_mailbox
        .map(|value| Uuid::parse_str(value).map_err(|_| StatusCode::BAD_REQUEST))
        .transpose()?;

    let matches: Vec<(Uuid, Uuid)> = match &state.db {
        DbPool::Postgres(pool) => {
            let rows = sqlx::query("SELECT id, tenant_id FROM mailboxes WHERE lower(address)=$1")
                .bind(&principal)
                .fetch_all(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            rows.into_iter()
                .map(|row| (row.get::<Uuid, _>("id"), row.get::<Uuid, _>("tenant_id")))
                .collect()
        }
        DbPool::Sqlite(pool) => {
            let rows = sqlx::query("SELECT id, tenant_id FROM mailboxes WHERE lower(address)=?")
                .bind(&principal)
                .fetch_all(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            rows.into_iter()
                .filter_map(|row| {
                    let mailbox_id = Uuid::parse_str(&row.get::<String, _>("id")).ok()?;
                    let tenant_id = Uuid::parse_str(&row.get::<String, _>("tenant_id")).ok()?;
                    Some((mailbox_id, tenant_id))
                })
                .collect()
        }
    };

    let (mailbox_id, tenant_id) = match matches.as_slice() {
        [(mailbox_id, tenant_id)] => (*mailbox_id, *tenant_id),
        [] => return Err(StatusCode::FORBIDDEN),
        _ => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    if requested.is_some_and(|requested| requested != mailbox_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    if let Some(claim_tenant) = claims.tenant_id.as_deref() {
        let claim_tenant = Uuid::parse_str(claim_tenant).map_err(|_| StatusCode::UNAUTHORIZED)?;
        if claim_tenant != tenant_id {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    Ok(ExecutionContext {
        tenant_id,
        mailbox_id,
        principal: principal.clone(),
        owner_type: OwnerType::User,
        caller_id: principal,
        audience: "aivory-mail-data-plane".to_string(),
        scopes: Vec::new(),
        grant_id: None,
        request_id: headers
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        trace_id: headers
            .get("x-trace-id")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
    })
}

/// MCP capability mode is intentionally disabled until Phase 2 implements
/// mailbox-scoped grants, audience/caller validation, expiry, replay checks,
/// and revocation. The explicit value prevents accidental enablement.
pub fn mcp_capability_mode_enabled() -> bool {
    std::env::var("AVRY_MCP_CAPABILITY_MODE")
        .map(|value| value.trim() == "v2")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::mcp_capability_mode_enabled;

    #[test]
    fn mcp_capability_mode_is_disabled_without_explicit_v2_value() {
        std::env::remove_var("AVRY_MCP_CAPABILITY_MODE");
        assert!(!mcp_capability_mode_enabled());
    }
}
