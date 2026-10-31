use crate::api::{authz, mcp_capabilities, AppState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use super::mcp_confirmations;

pub async fn issue(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<mcp_capabilities::IssueCapabilityRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    authz::require_admin(&state, &headers).await?;
    let (metadata, raw_token) = mcp_capabilities::issue_capability(&state, request).await?;
    Ok((
        StatusCode::CREATED,
        Json(mcp_capabilities::issue_body_to_value(&metadata, &raw_token)),
    ))
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    authz::require_admin(&state, &headers).await?;
    let grants = mcp_capabilities::list_capabilities(&state).await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "data": grants,
    })))
}

pub async fn issue_send_confirmation(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<mcp_confirmations::IssueSendConfirmationRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    authz::require_admin(&state, &headers).await?;
    let confirmation = mcp_confirmations::issue_send_confirmation(&state, request).await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "success": true,
        "confirmation": confirmation,
    }))))
}


pub async fn revoke(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    authz::require_admin(&state, &headers).await?;
    let grant_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    mcp_capabilities::revoke_capability(&state, grant_id).await?;
    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Debug, Deserialize)]
pub struct IssueSelfCapabilityRequest {
    /// Admins only: issue for this mailbox (e.g. an agent's mailbox the admin
    /// manages). Anyone else gets 403 unless it is their own mailbox.
    pub mailbox_id: Option<String>,
    pub caller_id: Option<String>,
    pub scopes: Vec<String>,
    pub expires_in_seconds: Option<i64>,
}

/// The mailbox a `/v1/me/mcp/grants` call acts on: the caller's own, or — for
/// an admin — the one named by `mailbox_id` (same rule as `mailbox_scope`).
async fn grant_mailbox(
    state: &Arc<AppState>,
    headers: &axum::http::HeaderMap,
    email: &str,
    requested: Option<&str>,
) -> Result<Uuid, StatusCode> {
    let requested = requested.map(str::trim).filter(|value| !value.is_empty());
    match authz::mailbox_scope(state, headers, requested).await? {
        Some(mailbox_id) => Ok(mailbox_id),
        None => Ok(mcp_capabilities::resolve_own_mailbox(state, email).await?.1),
    }
}

/// Self-service capability issuance — no admin privilege required for the
/// caller's own mailbox. `tenant_id` is never taken from the request; an
/// admin may name another mailbox via `mailbox_id`.
pub async fn issue_self(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<IssueSelfCapabilityRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let email = authz::authenticated_email(&state, &headers)?;
    let mailbox_id = grant_mailbox(&state, &headers, &email, request.mailbox_id.as_deref()).await?;
    let (metadata, raw_token) = mcp_capabilities::issue_self_capability(
        &state,
        &email,
        mailbox_id,
        request.caller_id,
        request.scopes,
        request.expires_in_seconds,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(mcp_capabilities::issue_body_to_value(&metadata, &raw_token)),
    ))
}

/// Lists the grants of the caller's own mailbox (or, for an admin,
/// `?mailbox_id=`).
pub async fn list_self(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let email = authz::authenticated_email(&state, &headers)?;
    let mailbox_id =
        grant_mailbox(&state, &headers, &email, query.get("mailbox_id").map(String::as_str)).await?;
    let grants = mcp_capabilities::list_own_capabilities(&state, mailbox_id).await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "data": grants,
    })))
}

/// Revokes a grant only if it belongs to the logged-in mailbox owner —
/// enforced in `revoke_own_capability`, not here, so there is no window
/// where the ownership check could be skipped by a future caller of that
/// function.
pub async fn revoke_self(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let email = authz::authenticated_email(&state, &headers)?;
    let mailbox_id =
        grant_mailbox(&state, &headers, &email, query.get("mailbox_id").map(String::as_str)).await?;
    let grant_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    mcp_capabilities::revoke_own_capability(&state, mailbox_id, grant_id).await?;
    Ok(Json(serde_json::json!({"success": true})))
}
