use crate::api::AppState;
use axum::{extract::{Query, State}, http::{HeaderMap, StatusCode}, Json};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct ResolveQuery {
    pub to: String,
}

/// Called by the SMTP ingress at RCPT TO time so unknown recipients get a
/// real 550 instead of being accepted and stored under an orphaned mailbox.
/// Internal-token protected — never exposed to end users.
pub async fn resolve_recipient(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ResolveQuery>,
) -> Result<Json<Value>, StatusCode> {
    if !crate::auth::verify_internal_token(&headers, &state.config.internal_token) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let resolution = crate::mail::routing::resolve_recipient(&state, &q.to)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "success": true, "data": resolution })))
}
