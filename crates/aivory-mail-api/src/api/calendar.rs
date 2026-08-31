use std::sync::Arc;
use axum::{extract::{State, Query}, Json, http::StatusCode};
use serde_json::Value;
use crate::api::AppState;

pub async fn status(State(_state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    match crate::calendar::get_status().await {
        Ok(v) => Ok(Json(serde_json::json!({"success": true, "data": v, "base_url": std::env::var("CALNODE_URL").unwrap_or_else(|_| "https://book.aivory.uk".into())}))),
        Err(e) => Ok(Json(serde_json::json!({"success": false, "error": e.to_string(), "hint": "Set CALNODE_URL=http://aivory-cal:3000 on VPS or https://book.aivory.uk locally, and CALNODE_API_KEY if needed"}))),
    }
}

pub async fn event_types(State(_state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    match crate::calendar::list_event_types().await {
        Ok(v) => Ok(Json(serde_json::json!({"success": true, "data": v}))),
        Err(e) => Err(StatusCode::BAD_GATEWAY),
    }
}

pub async fn slots(State(_state): State<Arc<AppState>>, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let slug = q.get("event_type_slug").or_else(|| q.get("slug")).and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let from = q.get("from").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let to = q.get("to").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let tz = q.get("tz").and_then(|v| v.as_str()).unwrap_or("UTC");
    match crate::calendar::get_slots(slug, from, to, tz).await {
        Ok(v) => Ok(Json(serde_json::json!({"success": true, "data": v}))),
        Err(_) => Err(StatusCode::BAD_GATEWAY),
    }
}

pub async fn create_booking(State(_state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    match crate::calendar::create_booking(body).await {
        Ok(v) => Ok(Json(v)),
        Err(e) => Ok(Json(serde_json::json!({"success": false, "error": e.to_string()}))),
    }
}

// Propose times from email intelligence — heuristic: extract dates and suggest CalNode slots
pub async fn propose(State(_state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let subject = body.get("subject").and_then(|v| v.as_str()).unwrap_or("");
    let text = body.get("body").or_else(|| body.get("text")).and_then(|v| v.as_str()).unwrap_or("");
    let combined = format!("{} {}", subject, text);
    // naive date extraction
    let has_meeting = combined.to_lowercase().contains("meeting") || combined.to_lowercase().contains("schedule") || combined.to_lowercase().contains("available");
    let suggested = if has_meeting {
        serde_json::json!([
            {"label": "Tomorrow 10:00 AM GST", "iso": chrono::Utc::now().checked_add_signed(chrono::Duration::days(1)).unwrap().to_rfc3339(), "url": format!("{}/book/aivory-call", std::env::var("CALNODE_URL").unwrap_or_else(|_| "https://book.aivory.uk".into()))},
            {"label": "Thursday 2:00 PM GST", "iso": chrono::Utc::now().checked_add_signed(chrono::Duration::days(3)).unwrap().to_rfc3339(), "url": format!("{}/book/aivory-call", std::env::var("CALNODE_URL").unwrap_or_else(|_| "https://book.aivory.uk".into()))}
        ])
    } else { serde_json::json!([]) };
    Ok(Json(serde_json::json!({"success": true, "data": {"needs_scheduling": has_meeting, "suggested_slots": suggested, "cal_url": std::env::var("CALNODE_URL").unwrap_or_else(|_| "https://book.aivory.uk".into())}})))
}
