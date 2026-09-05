use std::sync::Arc;
use axum::{extract::{State, Request}, http::{StatusCode, HeaderMap}, middleware::Next, response::Response};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    email: String,
    exp: usize,
    iat: usize,
}

/// The logged-in mailbox from the bearer JWT, lowercased. Anonymous or
/// malformed/expired tokens return 401 — every admin-only endpoint needs a
/// real identity to check against, not just "some token was present".
pub fn authenticated_email(state: &Arc<AppState>, headers: &HeaderMap) -> Result<String, StatusCode> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
        &Validation::default(),
    ).map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok(data.claims.email.to_lowercase())
}

/// Each domain has exactly one admin mailbox (`domains.admin_email`) — the
/// only account allowed into the admin console and allowed to read/manage
/// mailboxes other than its own on that domain. Falls back to the
/// instance-wide MAIL_ADMIN_EMAIL/SUPERADMIN_EMAIL for ops access when no
/// domain has an admin assigned yet.
pub async fn is_admin(state: &Arc<AppState>, email: &str) -> bool {
    let admin_email = state.config.mail_admin_email.to_lowercase();
    let superadmin_email = std::env::var("SUPERADMIN_EMAIL").unwrap_or_else(|_| "irfan.reichmann@aivory.uk".into()).to_lowercase();
    if email == admin_email || email == superadmin_email {
        return true;
    }
    let found: Option<String> = match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("SELECT admin_email FROM domains WHERE lower(admin_email)=$1 LIMIT 1")
                .bind(email).fetch_optional(pool).await.ok().flatten()
                .and_then(|r| r.get::<Option<String>, _>("admin_email"))
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("SELECT admin_email FROM domains WHERE lower(admin_email)=? LIMIT 1")
                .bind(email).fetch_optional(pool).await.ok().flatten()
                .and_then(|r| r.get::<Option<String>, _>("admin_email"))
        }
    };
    found.is_some()
}

pub async fn require_admin(state: &Arc<AppState>, headers: &HeaderMap) -> Result<String, StatusCode> {
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
pub async fn require_admin_mw(State(state): State<Arc<AppState>>, headers: HeaderMap, req: Request, next: Next) -> Result<Response, StatusCode> {
    require_admin(&state, &headers).await?;
    Ok(next.run(req).await)
}
