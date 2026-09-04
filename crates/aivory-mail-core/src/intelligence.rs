use crate::types::*;

/// Lightweight heuristic intelligence — in production this calls AI Gateway (OpenRouter / Cerveau).
/// This provides offline fallback + structure for the AI layer.

pub fn classify_intent(subject: &str, body: &str) -> String {
    let combined = format!("{} {}", subject, body).to_lowercase();
    if combined.contains("invoice") || combined.contains("payment") || combined.contains("overdue") { return "invoice".into(); }
    if combined.contains("meeting") || combined.contains("schedule") || combined.contains("calendar") { return "meeting_request".into(); }
    if combined.contains("support") || combined.contains("help") || combined.contains("issue") { return "support".into(); }
    if combined.contains("order") || combined.contains("purchase") { return "order".into(); }
    if combined.contains("unsubscribe") { return "marketing".into(); }
    "general".into()
}

pub fn detect_urgency(subject: &str, body: &str) -> Urgency {
    let c = format!("{} {}", subject, body).to_lowercase();
    if c.contains("urgent") || c.contains("asap") || c.contains("critical") || c.contains("overdue") { return Urgency::High; }
    if c.contains("today") || c.contains("deadline") || c.contains("due") { return Urgency::Medium; }
    Urgency::Low
}

pub fn extract_entities_heuristic(body: &str) -> Vec<Entity> {
    let mut entities = Vec::new();
    // very light: find amounts like AED 18,500 or $123
    for cap in regex_amounts(body) {
        entities.push(Entity { kind: "amount".into(), value: cap, confidence: 0.85 });
    }
    // invoice numbers
    for cap in regex_invoices(body) {
        entities.push(Entity { kind: "invoice".into(), value: cap, confidence: 0.9 });
    }
    entities
}

fn regex_amounts(text: &str) -> Vec<String> {
    let re = regex::Regex::new(r"(?i)(?:AED|\$|USD|EUR)\s?[\d,]+(?:\.\d{2})?").unwrap();
    re.find_iter(text).map(|m| m.as_str().to_string()).collect()
}
fn regex_invoices(text: &str) -> Vec<String> {
    let re = regex::Regex::new(r"(?i)invoice\s*#?\s*(\d+)").unwrap();
    re.captures_iter(text).filter_map(|c| c.get(0).map(|m| m.as_str().to_string())).collect()
}

pub fn suggest_actions(intent: &str, urgency: &Urgency) -> Vec<SuggestedAction> {
    match intent {
        "invoice" => vec![
            SuggestedAction { action: "create_task".into(), label: "Create Finance Task".into(), params: serde_json::json!({"queue": "finance"}), requires_approval: false },
            SuggestedAction { action: "draft_reply".into(), label: "Draft payment reminder".into(), params: serde_json::json!({}), requires_approval: true },
            SuggestedAction { action: "update_crm".into(), label: "Update CRM".into(), params: serde_json::json!({}), requires_approval: false },
        ],
        "meeting_request" => vec![
            SuggestedAction { action: "create_calendar_event".into(), label: "Create Calendar Event".into(), params: serde_json::json!({}), requires_approval: true },
            SuggestedAction { action: "draft_reply".into(), label: "Draft reply with availability".into(), params: serde_json::json!({}), requires_approval: true },
        ],
        _ if *urgency == Urgency::High => vec![
            SuggestedAction { action: "notify".into(), label: "Notify assignee".into(), params: serde_json::json!({}), requires_approval: false },
        ],
        _ => vec![],
    }
}

pub fn analyze(subject: &str, body: &str) -> IntelligenceResult {
    let intent = classify_intent(subject, body);
    let urgency = detect_urgency(subject, body);
    let entities = extract_entities_heuristic(body);
    let suggested_actions = suggest_actions(&intent, &urgency);
    let summary = if body.chars().count() > 200 { format!("{}…", body.chars().take(200).collect::<String>()) } else { body.to_string() };
    IntelligenceResult { summary, intent, urgency, entities, suggested_actions, language: "en".into() }
}
