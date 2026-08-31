use anyhow::Result;
use tracing::{info, warn};

fn cognee_url() -> Option<String> {
    std::env::var("COGNEE_URL")
        .or_else(|_| std::env::var("COGNEE_CERVEAU_URL"))
        .or_else(|_| std::env::var("GRAPH_URL"))
        .ok()
        .filter(|s| !s.trim().is_empty())
}
fn cognee_secret() -> Option<String> {
    std::env::var("COGNEE_INTERNAL_SECRET")
        .or_else(|_| std::env::var("X_CERVEAU_INTERNAL_SECRET"))
        .or_else(|_| std::env::var("COGNEE_SECRET"))
        .ok()
        .filter(|s| !s.trim().is_empty())
}

pub async fn remember_email(tenant_user_id: &str, agent_type: &str, subject: &str, body: &str, message_id: &str) -> Result<()> {
    let base = match cognee_url() {
        Some(u) => u.trim_end_matches('/').to_string(),
        None => {
            info!("cognee disabled (COGNEE_URL not set) — skip graph_remember for {}", message_id);
            return Ok(());
        }
    };
    let secret = cognee_secret();
    let text = format!("Subject: {}\nFrom thread: {}\n\n{}", subject, message_id, body);
    let client = reqwest::Client::new();
    // POST /api/v1/add — multipart with text field; sidecar derives UUIDv5 from headers
    let mut req = client
        .post(format!("{}/api/v1/add", base))
        .header("X-Tenant-Id", tenant_user_id)
        .header("X-Agent-Type", agent_type);
    if let Some(s) = secret.as_deref() {
        req = req.header("X-Cerveau-Internal-Secret", s);
    }
    // cognee expects multipart; send as form with dataset implicit (cerveau_graph fixed)
    let form = reqwest::multipart::Form::new()
        .text("data", text.clone())
        .text("dataset", "cerveau_graph".to_string());
    // Try multipart first
    let resp = req.multipart(form).send().await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            warn!("cognee add multipart failed ({}), try json fallback: {}", base, e);
            let mut req2 = client
                .post(format!("{}/api/v1/add", base))
                .header("X-Tenant-Id", tenant_user_id)
                .header("X-Agent-Type", agent_type)
                .header("Content-Type", "application/json");
            if let Some(s) = secret.as_deref() { req2 = req2.header("X-Cerveau-Internal-Secret", s); }
            let r2 = req2.json(&serde_json::json!({"text": text, "dataset": "cerveau_graph"})).send().await?;
            r2
        }
    };
    if !resp.status().is_success() {
        let txt = resp.text().await.unwrap_or_default();
        warn!("cognee add non-200 from {}: {}", base, txt);
        // don't fail ingestion
        return Ok(());
    }
    info!("cognee add ok for {} via {}", message_id, base);
    // Trigger cognify (build graph)
    let mut req2 = client
        .post(format!("{}/api/v1/cognify", base))
        .header("X-Tenant-Id", tenant_user_id)
        .header("X-Agent-Type", agent_type);
    if let Some(s) = secret.as_deref() {
        req2 = req2.header("X-Cerveau-Internal-Secret", s);
    }
    let resp2 = req2
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({"dataset": "cerveau_graph"}))
        .send()
        .await;
    match resp2 {
        Ok(r) if r.status().is_success() => info!("cognee cognify ok for {}", message_id),
        Ok(r) => warn!("cognee cognify non-200: {}", r.text().await.unwrap_or_default()),
        Err(e) => warn!("cognee cognify failed: {}", e),
    }
    Ok(())
}
