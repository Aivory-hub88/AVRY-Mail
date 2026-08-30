use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

/// Cloudflare Email Routing helpers — mirrors Mailflare's provisioning logic
/// but reimplemented in Rust for Aivory.

pub struct CfClient {
    token: String,
    client: Client,
}

impl CfClient {
    pub fn new(token: String) -> Self { Self { token, client: Client::new() } }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.bearer_auth(&self.token)
    }

    pub async fn enable_email_routing(&self, zone_id: &str) -> Result<Value> {
        let resp = self.auth(self.client.post(format!("https://api.cloudflare.com/client/v4/zones/{}/email/routing/dns", zone_id)))
            .json(&serde_json::json!({})).send().await?;
        let v: Value = resp.json().await?;
        Ok(v)
    }

    pub async fn list_routing_rules(&self, zone_id: &str) -> Result<Value> {
        let resp = self.auth(self.client.get(format!("https://api.cloudflare.com/client/v4/zones/{}/email/routing/rules", zone_id)))
            .send().await?;
        Ok(resp.json().await?)
    }

    pub async fn create_routing_rule(&self, zone_id: &str, address: &str, worker_name: &str) -> Result<Value> {
        // action: forward to worker
        let payload = serde_json::json!({
            "matchers": [{"type": "literal", "field": "to", "value": address}],
            "actions": [{"type": "forward", "value": [worker_name]}],
            "enabled": true,
            "name": format!("aivory-mail {}", address),
            "priority": 0,
        });
        let resp = self.auth(self.client.post(format!("https://api.cloudflare.com/client/v4/zones/{}/email/routing/rules", zone_id)))
            .json(&payload).send().await?;
        Ok(resp.json().await?)
    }

    pub async fn delete_routing_rule(&self, zone_id: &str, rule_id: &str) -> Result<Value> {
        let resp = self.auth(self.client.delete(format!("https://api.cloudflare.com/client/v4/zones/{}/email/routing/rules/{}", zone_id, rule_id)))
            .send().await?;
        Ok(resp.json().await?)
    }

    pub async fn get_dns_records(&self, zone_id: &str) -> Result<Value> {
        let resp = self.auth(self.client.get(format!("https://api.cloudflare.com/client/v4/zones/{}/email/routing/dns", zone_id)))
            .send().await?;
        Ok(resp.json().await?)
    }

    pub async fn create_sending_subdomain(&self, zone_id: &str, subdomain: &str) -> Result<Value> {
        let resp = self.auth(self.client.post(format!("https://api.cloudflare.com/client/v4/zones/{}/email/sending/subdomains", zone_id)))
            .json(&serde_json::json!({"subdomain": subdomain})).send().await?;
        Ok(resp.json().await?)
    }
}
