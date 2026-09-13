use crate::api::{authz, mcp_capabilities, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::Value;
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
    pub caller_id: Option<String>,
    pub scopes: Vec<String>,
    pub expires_in_seconds: Option<i64>,
}

/// Self-service capability issuance for the logged-in mailbox owner — no
/// admin privilege required. `tenant_id`/`mailbox_id` are never taken from
/// the request; `issue_self_capability` resolves them from the caller's own
/// authenticated email, so this can only ever grant access to the caller's
/// own mailbox.
pub async fn issue_self(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<IssueSelfCapabilityRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let email = authz::authenticated_email(&state, &headers)?;
    let (metadata, raw_token) = mcp_capabilities::issue_self_capability(
        &state,
        &email,
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

/// Lists only the grants belonging to the logged-in mailbox owner.
pub async fn list_self(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    let email = authz::authenticated_email(&state, &headers)?;
    let (_, mailbox_id) = mcp_capabilities::resolve_own_mailbox(&state, &email).await?;
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
) -> Result<Json<Value>, StatusCode> {
    let email = authz::authenticated_email(&state, &headers)?;
    let (_, mailbox_id) = mcp_capabilities::resolve_own_mailbox(&state, &email).await?;
    let grant_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    mcp_capabilities::revoke_own_capability(&state, mailbox_id, grant_id).await?;
    Ok(Json(serde_json::json!({"success": true})))
}
