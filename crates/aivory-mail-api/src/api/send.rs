use std::sync::Arc;
use axum::{extract::State, Json, http::{HeaderMap, StatusCode}};
use serde_json::Value;
use crate::api::AppState;
use crate::mail::send_guard::{self, SendClaim};
use aivory_mail_core::types::SendRequest;
use uuid::Uuid;

fn failure(status: StatusCode, error: &str) -> (StatusCode, Json<Value>) {
    (status, Json(serde_json::json!({"success": false, "error": error})))
}

fn idempotency_failure(e: send_guard::SendClaimError) -> (StatusCode, Json<Value>) {
    (StatusCode::CONFLICT, Json(serde_json::json!({"success": false, "code": e.code(), "error": e.message()})))
}

/// Dispatch one send, at most once per `Idempotency-Key` when the caller gave
/// one. Shared by `/v1/send` and the thread reply endpoint.
pub(crate) async fn send_once(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    req: SendRequest,
) -> Result<Uuid, (StatusCode, Json<Value>)> {
    let key = send_guard::read_idempotency_key(headers).map_err(|_| {
        failure(StatusCode::BAD_REQUEST, "Idempotency-Key must be 8-200 characters of A-Z a-z 0-9 . _ : -")
    })?;
    let attempt_id = match key {
        None => None,
        Some(key) => {
            let caller = crate::api::authz::authenticated_email(state, headers)
                .map_err(|status| failure(status, "missing or invalid user session"))?;
            let claim = send_guard::claim_send(state, &caller, &key, &req).await.map_err(|e| {
                tracing::error!("send_attempts claim failed: {}", e);
                failure(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
            })?;
            match claim.map_err(idempotency_failure)? {
                SendClaim::Replay { message_id } => {
                    return Uuid::parse_str(&message_id)
                        .map_err(|_| failure(StatusCode::INTERNAL_SERVER_ERROR, "internal error"));
                }
                SendClaim::Claimed { attempt_id } => Some(attempt_id),
            }
        }
    };

    // The actual SMTP/Cloudflare round-trip takes several seconds. Run it in
    // a detached task so a client that closes the tab or navigates away
    // mid-request (HTTP connection dropped -> our future would otherwise be
    // cancelled) can never abort a send after it already left the building —
    // the message would go out to the recipient but silently never get
    // written to the Sent folder. Dropping the JoinHandle does not stop the
    // spawned task; only the connection-bound future above it goes away. The
    // idempotency outcome is recorded inside the task for the same reason.
    let task_state = state.clone();
    let handle = tokio::spawn(async move {
        let result = crate::mail::outbound::send_email(&task_state, req).await;
        if let Some(attempt_id) = attempt_id {
            send_guard::finish_send(&task_state, &attempt_id, result.as_ref().ok().copied()).await;
        }
        result
    });
    match handle.await {
        Ok(Ok(id)) => Ok(id),
        Ok(Err(e)) => {
            tracing::error!("send failed: {}", e);
            Err(failure(StatusCode::BAD_REQUEST, &e.to_string()))
        }
        Err(join_err) => {
            tracing::error!("send task panicked: {}", join_err);
            Err(failure(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))
        }
    }
}

pub async fn send_email(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let req: SendRequest = serde_json::from_value(body).map_err(|e| { tracing::error!("invalid send payload: {}", e); StatusCode::BAD_REQUEST })?;
    if let Err(e) = aivory_mail_core::routing::validate_send_request(&req) {
        return Ok(failure(StatusCode::BAD_REQUEST, &e.to_string()));
    }
    // The session only proves who the caller is; `from` is theirs to use only
    // if it is their own address, one of their Send As aliases, or they are
    // an instance admin.
    if let Err(e) = crate::mail::outbound::authorize_sender(&state, &headers, &req).await {
        return Ok(failure(StatusCode::FORBIDDEN, &e.to_string()));
    }
    match send_once(&state, &headers, req).await {
        Ok(id) => Ok((StatusCode::ACCEPTED, Json(serde_json::json!({"success": true, "data": {"id": id.to_string(), "status": "queued"}})))),
        Err(response) => Ok(response),
    }
}

pub async fn send_batch(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let reqs: Vec<SendRequest> = serde_json::from_value(body.get("messages").cloned().unwrap_or(Value::Null))
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    if reqs.len() > 50 { return Err(StatusCode::BAD_REQUEST); }
    // Authorize the whole batch before sending any of it, so a batch is never
    // half-sent because one entry used someone else's address.
    for req in &reqs {
        if crate::mail::outbound::authorize_sender(&state, &headers, req).await.is_err() {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    let mut results = Vec::new();
    for req in reqs {
        match crate::mail::outbound::send_email(&state, req).await {
            Ok(id) => results.push(serde_json::json!({"id": id.to_string(), "status": "queued"})),
            Err(e) => results.push(serde_json::json!({"error": e.to_string(), "status": "failed"})),
        }
    }
    Ok(Json(serde_json::json!({"success": true, "data": results})))
}
