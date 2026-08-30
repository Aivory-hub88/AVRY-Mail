use std::sync::Arc;
use axum::{extract::State, Json, http::StatusCode, body::Bytes};
use serde_json::Value;
use crate::api::AppState;

pub async fn inbound(State(state): State<Arc<AppState>>, body: Bytes) -> Result<Json<Value>, StatusCode> {
    // Generic inbound webhook: expects JSON {from, to, raw_base64} or raw bytes
    // Try parse as JSON first
    if let Ok(json) = serde_json::from_slice::<Value>(&body) {
        let from = json.get("from").and_then(|v| v.as_str()).unwrap_or("unknown@unknown");
        let to = json.get("to").and_then(|v| v.as_str()).unwrap_or("unknown@unknown");
        let raw_b64 = json.get("raw").or_else(|| json.get("raw_base64")).and_then(|v| v.as_str());
        let raw = if let Some(b64) = raw_b64 {
            use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
            B64.decode(b64.trim()).map_err(|_| StatusCode::BAD_REQUEST)?
        } else if let Some(raw_str) = json.get("raw_text").and_then(|v| v.as_str()) {
            raw_str.as_bytes().to_vec()
        } else {
            // no raw, synthesize minimal email
            let subject = json.get("subject").and_then(|v| v.as_str()).unwrap_or("(no subject)");
            let text = json.get("text").and_then(|v| v.as_str()).unwrap_or("");
            format!("From: {}\r\nTo: {}\r\nSubject: {}\r\n\r\n{}", from, to, subject, text).into_bytes()
        };
        match crate::mail::inbound::handle_inbound_raw(&state, from, to, raw).await {
            Ok(id) => Ok(Json(serde_json::json!({"success": true, "id": id.to_string()}))),
            Err(e) => { tracing::error!("inbound handle failed: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
        }
    } else {
        // raw MIME bytes
        let raw = body.to_vec();
        // try to extract From/To from raw via parser
        let parsed = aivory_mail_core::parser::parse_raw_email(&raw).map_err(|_| StatusCode::BAD_REQUEST)?;
        let from = parsed.from_addr.clone().unwrap_or_else(|| "unknown@unknown".into());
        let to = parsed.to_addrs.first().cloned().unwrap_or_else(|| "unknown@unknown".into());
        match crate::mail::inbound::handle_inbound_raw(&state, &from, &to, raw).await {
            Ok(id) => Ok(Json(serde_json::json!({"success": true, "id": id.to_string()}))),
            Err(e) => { tracing::error!("inbound raw handle failed: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
        }
    }
}

/// Cloudflare Email Routing forward: receives POST from Cloudflare Worker email() handler
pub async fn cloudflare_email(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let from = body.get("from").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let to = body.get("to").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let raw_b64 = body.get("raw").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    let raw = B64.decode(raw_b64.trim()).map_err(|_| StatusCode::BAD_REQUEST)?;
    match crate::mail::inbound::handle_inbound_raw(&state, from, to, raw).await {
        Ok(id) => Ok(Json(serde_json::json!({"success": true, "id": id.to_string()}))),
        Err(e) => { tracing::error!("cf email handle failed: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
    }
}
