use std::sync::Arc;
use axum::{extract::State, Json, http::{HeaderMap, StatusCode}, body::Bytes};
use serde_json::Value;
use crate::api::AppState;

/// How the inbound webhooks treat a request without a valid internal token.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum WebhookAuthMode {
    /// Reject with 401. The default, and what every deployment should run.
    Enforce,
    /// Accept, but log loudly whether the token was missing or wrong. A
    /// rollout stage only: it shows which real callers (the SMTP ingress, the
    /// Cloudflare worker) still send a bad token before they get locked out.
    Warn,
}

/// `INBOUND_WEBHOOK_AUTH=warn` selects [`WebhookAuthMode::Warn`]; anything
/// else, including unset, enforces.
pub fn auth_mode_from(value: Option<&str>) -> WebhookAuthMode {
    match value.map(|v| v.trim().to_lowercase()).as_deref() {
        Some("warn" | "log") => WebhookAuthMode::Warn,
        _ => WebhookAuthMode::Enforce,
    }
}

/// Gate for the inbound webhooks. They sit on the public router (the SMTP
/// ingress and the Cloudflare worker have no user session), so this is their
/// only line of defence: without it anyone who can reach the API could plant
/// mail in any mailbox, or pick the target folder, past filters and blocks.
fn authorize_webhook(state: &Arc<AppState>, headers: &HeaderMap, route: &str) -> Result<(), StatusCode> {
    authorize_webhook_with(
        crate::auth::verify_internal_token(headers, &state.config.internal_token),
        headers.contains_key("x-internal-token"),
        auth_mode_from(std::env::var("INBOUND_WEBHOOK_AUTH").ok().as_deref()),
        route,
    )
}

fn authorize_webhook_with(valid: bool, token_sent: bool, mode: WebhookAuthMode, route: &str) -> Result<(), StatusCode> {
    if valid {
        tracing::info!("webhook {}: internal token ok", route);
        return Ok(());
    }
    let why = if token_sent { "token does not match INTERNAL_TOKEN" } else { "no x-internal-token header" };
    match mode {
        WebhookAuthMode::Enforce => {
            tracing::warn!("webhook {}: REJECTED ({})", route, why);
            Err(StatusCode::UNAUTHORIZED)
        }
        WebhookAuthMode::Warn => {
            tracing::warn!("webhook {}: ACCEPTED WITHOUT A VALID TOKEN ({}) — INBOUND_WEBHOOK_AUTH=warn", route, why);
            Ok(())
        }
    }
}

pub async fn inbound(State(state): State<Arc<AppState>>, headers: HeaderMap, body: Bytes) -> Result<Json<Value>, StatusCode> {
    authorize_webhook(&state, &headers, "inbound")?;
    // Generic inbound webhook: expects JSON {from, to, raw_base64, folder} or raw bytes
    // Try parse as JSON first
    if let Ok(json) = serde_json::from_slice::<Value>(&body) {
        let from = json.get("from").and_then(|v| v.as_str()).unwrap_or("unknown@unknown");
        let to = json.get("to").and_then(|v| v.as_str()).unwrap_or("unknown@unknown");
        let forced_folder = json.get("folder").and_then(|v| v.as_str()).map(|s| s.to_string());
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
        match crate::mail::inbound::handle_inbound_raw_with_folder(&state, from, to, raw, forced_folder).await {
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
pub async fn cloudflare_email(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    authorize_webhook(&state, &headers, "cloudflare")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_defaults_to_enforce() {
        assert_eq!(auth_mode_from(None), WebhookAuthMode::Enforce);
        assert_eq!(auth_mode_from(Some("")), WebhookAuthMode::Enforce);
        assert_eq!(auth_mode_from(Some("enforce")), WebhookAuthMode::Enforce);
        assert_eq!(auth_mode_from(Some("off")), WebhookAuthMode::Enforce); // no way to switch it off
        assert_eq!(auth_mode_from(Some(" WARN ")), WebhookAuthMode::Warn);
        assert_eq!(auth_mode_from(Some("log")), WebhookAuthMode::Warn);
    }

    #[test]
    fn decision_table() {
        let ok = |valid, sent, mode| authorize_webhook_with(valid, sent, mode, "t");
        assert_eq!(ok(true, true, WebhookAuthMode::Enforce), Ok(()));
        assert_eq!(ok(false, true, WebhookAuthMode::Enforce), Err(StatusCode::UNAUTHORIZED));
        assert_eq!(ok(false, false, WebhookAuthMode::Enforce), Err(StatusCode::UNAUTHORIZED));
        assert_eq!(ok(false, true, WebhookAuthMode::Warn), Ok(()));
        assert_eq!(ok(false, false, WebhookAuthMode::Warn), Ok(()));
    }
}
