use crate::api::{authz, mcp_capabilities, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
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
