use std::sync::Arc;
use axum::{extract::State, Json, http::StatusCode};
use serde_json::Value;
use crate::api::AppState;

pub async fn analyze(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let subject = body.get("subject").and_then(|v| v.as_str()).unwrap_or("");
    let body_text = body.get("body").or_else(|| body.get("text")).and_then(|v| v.as_str()).unwrap_or("");
    let heuristic = aivory_mail_core::intelligence::analyze(subject, body_text);

    // If AI gateway configured, try to enrich
    let mut result = serde_json::to_value(&heuristic).unwrap();
    if let Some(ai_url) = &state.config.ai_gateway_url {
        let payload = serde_json::json!({"subject": subject, "body": &body_text[..body_text.len().min(4000)], "heuristic": heuristic, "model": state.config.mail_intelligence_model});
        if let Ok(resp) = reqwest::Client::new().post(format!("{}/v1/ai/analyze-email", ai_url))
            .header("x-internal-token", &state.config.internal_token)
            .json(&payload).timeout(std::time::Duration::from_secs(8)).send().await
        {
            if let Ok(ai_json) = resp.json::<Value>().await {
                result["ai"] = ai_json;
            }
        }
    }
    Ok(Json(serde_json::json!({"success": true, "data": result})))
}

pub async fn suggest(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    // If subject/body provided, generate draft (AI killer feature)
    if let (Some(subject), Some(btext)) = (body.get("subject").and_then(|v| v.as_str()), body.get("body").and_then(|v| v.as_str())) {
        if !subject.is_empty() || !btext.is_empty() {
            if let Some(ai_url) = &state.config.ai_gateway_url {
                let payload = serde_json::json!({"subject": subject, "body": btext, "model": state.config.mail_intelligence_model});
                if let Ok(resp) = reqwest::Client::new().post(format!("{}/v1/ai/draft-reply", ai_url))
                    .header("x-internal-token", &state.config.internal_token)
                    .json(&payload).timeout(std::time::Duration::from_secs(8)).send().await
                {
                    if let Ok(ai_json) = resp.json::<Value>().await {
                        if let Some(draft) = ai_json.get("draft").and_then(|v| v.as_str()) {
                            return Ok(Json(serde_json::json!({"success": true, "data": {"draft": draft, "raw": ai_json}})));
                        }
                        return Ok(Json(serde_json::json!({"success": true, "data": {"draft": ai_json}})));
                    }
                }
            }
            // heuristic fallback draft
            let snippet = if btext.len() > 120 { &btext[..120] } else { btext };
            let draft = format!("Hi,\n\nThanks for your email regarding \"{}\".\n\nRe: {} — noted. I'll follow up shortly.\n\nBest regards", subject, snippet);
            return Ok(Json(serde_json::json!({"success": true, "data": {"draft": draft}})));
        }
    }
    let intent = body.get("intent").and_then(|v| v.as_str()).unwrap_or("general");
    let urgency_str = body.get("urgency").and_then(|v| v.as_str()).unwrap_or("low");
    let urgency = match urgency_str {
        "high" | "critical" => aivory_mail_core::types::Urgency::High,
        "medium" => aivory_mail_core::types::Urgency::Medium,
        _ => aivory_mail_core::types::Urgency::Low,
    };
    let actions = aivory_mail_core::intelligence::suggest_actions(intent, &urgency);
    Ok(Json(serde_json::json!({"success": true, "data": actions})))
}

pub async fn agent_actions(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let action = body.get("action").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let message_id = body.get("message_id").and_then(|v| v.as_str());
    tracing::info!("agent action {} for {:?}", action, message_id);

    // Dispatch to workflow / n8n / internal handlers
    let result = match action {
        "create_task" => {
            if let Some(wf) = &state.config.workflow_url {
                let _ = reqwest::Client::new().post(format!("{}/webhook/agent-action", wf))
                    .json(&body).send().await;
            }
            serde_json::json!({"status": "queued", "action": "create_task"})
        }
        "draft_reply" => {
            // Could call AI gateway to draft
            serde_json::json!({"status": "draft_created", "draft": "Draft reply placeholder — AI gateway will fill"})
        }
        "update_crm" | "notify_finance" | "send_reminder" => {
            serde_json::json!({"status": "queued", "action": action})
        }
        _ => serde_json::json!({"status": "unknown_action", "action": action}),
    };
    Ok(Json(serde_json::json!({"success": true, "data": result})))
}
