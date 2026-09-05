use std::sync::Arc;
use axum::{extract::State, Json, http::StatusCode};
use serde_json::Value;
use crate::api::AppState;
use aivory_mail_core::types::SendRequest;

pub async fn send_email(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let req: SendRequest = serde_json::from_value(body).map_err(|e| { tracing::error!("invalid send payload: {}", e); StatusCode::BAD_REQUEST })?;
    // The actual SMTP/Cloudflare round-trip takes several seconds. Run it in
    // a detached task so a client that closes the tab or navigates away
    // mid-request (HTTP connection dropped -> our future would otherwise be
    // cancelled) can never abort a send after it already left the building —
    // the message would go out to the recipient but silently never get
    // written to the Sent folder. Dropping the JoinHandle does not stop the
    // spawned task; only the connection-bound future above it goes away.
    let handle = tokio::spawn(async move { crate::mail::outbound::send_email(&state, req).await });
    match handle.await {
        Ok(Ok(id)) => Ok((StatusCode::ACCEPTED, Json(serde_json::json!({"success": true, "data": {"id": id.to_string(), "status": "queued"}})))),
        Ok(Err(e)) => {
            tracing::error!("send failed: {}", e);
            Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"success": false, "error": e.to_string()}))))
        }
        Err(join_err) => {
            tracing::error!("send task panicked: {}", join_err);
            Ok((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"success": false, "error": "internal error"}))))
        }
    }
}

pub async fn send_batch(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let reqs: Vec<SendRequest> = serde_json::from_value(body.get("messages").cloned().unwrap_or(Value::Null))
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    if reqs.len() > 50 { return Err(StatusCode::BAD_REQUEST); }
    let mut results = Vec::new();
    for req in reqs {
        match crate::mail::outbound::send_email(&state, req).await {
            Ok(id) => results.push(serde_json::json!({"id": id.to_string(), "status": "queued"})),
            Err(e) => results.push(serde_json::json!({"error": e.to_string(), "status": "failed"})),
        }
    }
    Ok(Json(serde_json::json!({"success": true, "data": results})))
}
