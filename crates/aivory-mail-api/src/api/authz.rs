use crate::{api::AppState, auth};
use aivory_mail_storage::db::DbPool;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode},
    middleware::Next,
    response::Response,
};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

/// The logged-in mailbox from the bearer JWT, lowercased. Anonymous or
/// malformed/expired tokens return 401 — every admin-only endpoint needs a
/// real identity to check against, not just "some token was present".
pub fn authenticated_email(
    state: &Arc<AppState>,
    headers: &HeaderMap,
) -> Result<String, StatusCode> {
    let token = auth::extract_bearer(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let claims =
        auth::verify_jwt(&token, &state.config.jwt_secret).map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok(claims.sub.trim().to_lowercase())
}

pub fn require_internal(state: &Arc<AppState>, headers: &HeaderMap) -> Result<(), StatusCode> {
    if auth::verify_internal_token(headers, &state.config.internal_token) {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Global route gate. Public exceptions are explicit and each has its own
/// handler-level validation; no internal token bypasses ordinary user APIs.
pub async fn require_user_mw(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();
    let mcp_internal = path == "/mcp"
        && (auth::verify_internal_token(&headers, &state.config.internal_token)
            || state
                .config
                .cognee_secret
                .as_deref()
                .is_some_and(|expected| {
                    headers
                        .get("x-cerveau-internal-secret")
                        .and_then(|v| v.to_str().ok())
                        == Some(expected)
                }));
    let public = path == "/health"
        || path == "/v1/health"
        || (path == "/v1/auth/login" && req.method() == Method::POST)
        || (path == "/v1/webhooks/inbound" && req.method() == Method::POST)
        || (path == "/v1/webhooks/cloudflare" && req.method() == Method::POST)
        || (path == "/v1/internal/resolve-recipient" && req.method() == Method::GET)
        || mcp_internal;
    if public {
        return Ok(next.run(req).await);
    }
    authenticated_email(&state, &headers)?;
    Ok(next.run(req).await)
}

async fn own_mailbox_id(state: &Arc<AppState>, email: &str) -> Result<Uuid, StatusCode> {
    let id = match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("SELECT id FROM mailboxes WHERE lower(address)=$1 LIMIT 1")
                .bind(email)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|row| row.get::<Uuid, _>("id"))
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("SELECT id FROM mailboxes WHERE lower(address)=? LIMIT 1")
                .bind(email)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .and_then(|row| Uuid::parse_str(&row.get::<String, _>("id")).ok())
        }
    };
    id.ok_or(StatusCode::FORBIDDEN)
}

/// Resolve a requested mailbox only for admins; ordinary users are always
/// constrained to the mailbox represented by their JWT.
pub async fn mailbox_scope(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    requested: Option<&str>,
) -> Result<Option<Uuid>, StatusCode> {
    let email = authenticated_email(state, headers)?;
    if is_admin(state, &email).await {
        return requested
            .map(|id| Uuid::parse_str(id).map_err(|_| StatusCode::BAD_REQUEST))
            .transpose();
    }
    let own = own_mailbox_id(state, &email).await?;
    if let Some(requested) = requested {
        if Uuid::parse_str(requested).ok() != Some(own) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(Some(own))
}

pub async fn require_message_access(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    message_id: Uuid,
) -> Result<(), StatusCode> {
    let email = authenticated_email(state, headers)?;
    if is_admin(state, &email).await {
        return Ok(());
    }
    let found = match &state.db {
        DbPool::Postgres(pool) => sqlx::query("SELECT 1 FROM messages m JOIN mailboxes b ON b.id=m.mailbox_id WHERE m.id=$1 AND lower(b.address)=$2").bind(message_id).bind(email).fetch_optional(pool).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.is_some(),
        DbPool::Sqlite(pool) => sqlx::query("SELECT 1 FROM messages m JOIN mailboxes b ON b.id=m.mailbox_id WHERE m.id=? AND lower(b.address)=?").bind(message_id.to_string()).bind(email).fetch_optional(pool).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.is_some(),
    };
    if found {
        Ok(())
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub async fn require_thread_access(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    thread_id: Uuid,
) -> Result<(), StatusCode> {
    let email = authenticated_email(state, headers)?;
    if is_admin(state, &email).await {
        return Ok(());
    }
    let found = match &state.db {
        DbPool::Postgres(pool) => sqlx::query("SELECT 1 FROM threads t JOIN mailboxes b ON b.id=t.mailbox_id WHERE t.id=$1 AND lower(b.address)=$2").bind(thread_id).bind(email).fetch_optional(pool).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.is_some(),
        DbPool::Sqlite(pool) => sqlx::query("SELECT 1 FROM threads t JOIN mailboxes b ON b.id=t.mailbox_id WHERE t.id=? AND lower(b.address)=?").bind(thread_id.to_string()).bind(email).fetch_optional(pool).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.is_some(),
    };
    if found {
        Ok(())
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Instance-wide administrator check. Domain-admin metadata is deliberately
/// not treated as a global grant; domain-scoped administration needs dedicated
/// per-domain query constraints before it can safely be enabled.
pub async fn is_admin(state: &Arc<AppState>, email: &str) -> bool {
    // Only the explicitly configured operations administrator and optional
    // superadmin may access instance-wide routes. A domain's admin_email is
    // not a global-admin grant: treating it as one would let an admin from
    // domain A enumerate or manage domain B.
    let admin_email = state.config.mail_admin_email.to_lowercase();
    let superadmin_emails: Vec<String> = std::env::var("SUPERADMIN_EMAIL")
        .unwrap_or_default()
        .split(',')
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect();
    email == admin_email || superadmin_emails.iter().any(|candidate| candidate == email)
}

pub async fn require_admin(
    state: &Arc<AppState>,
    headers: &HeaderMap,
) -> Result<String, StatusCode> {
    let email = authenticated_email(state, headers)?;
    if is_admin(state, &email).await {
        Ok(email)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// Route-layer gate for the admin-console-only endpoints (domains,
/// mailbox provisioning, groups, API keys, audit log, webhook registry) —
/// everything that lists or manages data across the whole instance rather
/// than a single mailbox.
pub async fn require_admin_mw(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    require_admin(&state, &headers).await?;
    Ok(next.run(req).await)
}
