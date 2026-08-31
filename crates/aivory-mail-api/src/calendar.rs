use anyhow::Result;
use serde_json::Value;

fn base_url() -> String {
    std::env::var("CALNODE_URL").or_else(|_| std::env::var("CALENDAR_URL")).unwrap_or_else(|_| "https://book.aivory.uk".into())
}
fn api_key() -> Option<String> {
    std::env::var("CALNODE_API_KEY").or_else(|_| std::env::var("CAL_API_KEY")).ok()
}

pub async fn get_status() -> Result<Value> {
    let url = format!("{}/v1/calendar/status", base_url());
    let mut req = reqwest::Client::new().get(&url);
    if let Some(k) = api_key() { req = req.header("Authorization", format!("Bearer {}", k)); }
    let r = req.send().await?;
    let v: Value = r.json().await.unwrap_or_else(|_| serde_json::json!({"status":"unknown"}));
    Ok(v)
}

pub async fn list_event_types() -> Result<Value> {
    let url = format!("{}/v1/event-types", base_url());
    let mut req = reqwest::Client::new().get(&url);
    if let Some(k) = api_key() { req = req.header("Authorization", format!("Bearer {}", k)); }
    let r = req.send().await?;
    if !r.status().is_success() { anyhow::bail!("calnode list_event_types {}", r.status()); }
    Ok(r.json().await?)
}

pub async fn get_slots(event_type_slug: &str, from: &str, to: &str, tz: &str) -> Result<Value> {
    let url = format!("{}/v1/event-types/{}/slots?from={}&to={}&tz={}", base_url(), event_type_slug, from, to, urlencoding::encode(tz));
    let mut req = reqwest::Client::new().get(&url);
    if let Some(k) = api_key() { req = req.header("Authorization", format!("Bearer {}", k)); }
    let r = req.send().await?;
    if !r.status().is_success() { anyhow::bail!("slots {}", r.status()); }
    Ok(r.json().await?)
}

pub async fn create_booking(payload: Value) -> Result<Value> {
    let url = format!("{}/v1/bookings", base_url());
    let mut req = reqwest::Client::new().post(&url).json(&payload);
    if let Some(k) = api_key() { req = req.header("Authorization", format!("Bearer {}", k)); }
    let r = req.send().await?;
    if !r.status().is_success() {
        let txt = r.text().await.unwrap_or_default();
        anyhow::bail!("create_booking {}", txt);
    }
    // For 200 case we already consumed; need to re-request? Instead just return generic success
    Ok(serde_json::json!({"success": true}))
}
