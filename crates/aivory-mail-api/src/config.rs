use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub storage_backend: String, // local | r2 | s3
    pub storage_bucket: String,
    pub storage_path: String,
    pub jwt_secret: String,
    pub internal_token: String,
    pub mail_mode: String, // cloudflare | vps | hybrid
    pub cf_api_token: Option<String>,
    pub cf_zone_id: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub ai_gateway_url: Option<String>,
    pub workflow_url: Option<String>,
    pub cors_origins: Vec<String>,
    pub r2_endpoint: Option<String>,
    pub r2_access_key: Option<String>,
    pub r2_secret_key: Option<String>,
    pub mail_intelligence_model: String,
    pub diagnostic_model: String,
    pub cognee_url: Option<String>,
    pub cognee_secret: Option<String>,
    pub cognee_agent_type: String,
    /// Hostname customers point their domain's MX record at (this VPS's SMTP ingress).
    pub mail_mx_host: String,
    /// Hostname referenced by the SPF `include:` mechanism in customer SPF records —
    /// Aivory's own domain publishes the actual sending-IP TXT record there.
    pub spf_include_host: String,
    pub dmarc_report_address: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            port: env::var("PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(8095),
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite::memory:".into()),
            storage_backend: env::var("STORAGE_BACKEND").unwrap_or_else(|_| "local".into()),
            storage_bucket: env::var("STORAGE_BUCKET").unwrap_or_else(|_| "aivory-mail".into()),
            storage_path: env::var("STORAGE_PATH").unwrap_or_else(|_| "./data/mail-storage".into()),
            jwt_secret: env::var("JWT_SECRET").unwrap_or_else(|_| "aivory-mail-dev-secret-change-me".into()),
            internal_token: env::var("INTERNAL_TOKEN").unwrap_or_else(|_| "aivory-internal-dev".into()),
            mail_mode: env::var("MAIL_MODE").unwrap_or_else(|_| "vps".into()),
            cf_api_token: env::var("CF_API_TOKEN").ok(),
            cf_zone_id: env::var("CF_ZONE_ID").ok(),
            smtp_host: env::var("SMTP_HOST").ok(),
            smtp_port: env::var("SMTP_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(587),
            ai_gateway_url: env::var("AI_GATEWAY_URL").or_else(|_| env::var("ZEROCLAW_URL")).ok(),
            workflow_url: env::var("WORKFLOW_URL").or_else(|_| env::var("N8N_AS_CODE_URL")).ok(),
            cors_origins: env::var("CORS_ORIGINS").unwrap_or_else(|_| "http://localhost:9000,http://localhost:9001".into())
                .split(',').map(|s| s.trim().to_string()).collect(),
            r2_endpoint: env::var("R2_ENDPOINT").ok(),
            r2_access_key: env::var("R2_ACCESS_KEY_ID").ok(),
            r2_secret_key: env::var("R2_SECRET_ACCESS_KEY").ok(),
            mail_intelligence_model: env::var("MAIL_INTELLIGENCE_MODEL").unwrap_or_else(|_| "deepseek/deepseek-v4-flash-0731".into()),
            diagnostic_model: env::var("DIAGNOSTIC_MODEL").unwrap_or_else(|_| "qwen/qwen3-235b-a22b".into()),
            cognee_url: env::var("COGNEE_URL").or_else(|_| env::var("COGNEE_CERVEAU_URL")).ok(),
            cognee_secret: env::var("COGNEE_INTERNAL_SECRET").or_else(|_| env::var("X_CERVEAU_INTERNAL_SECRET")).ok(),
            cognee_agent_type: env::var("COGNEE_AGENT_TYPE").unwrap_or_else(|_| "mail_ops".into()),
            mail_mx_host: env::var("MAIL_MX_HOST").unwrap_or_else(|_| "mail.aivory.id".into()),
            spf_include_host: env::var("SPF_INCLUDE_HOST").unwrap_or_else(|_| "_spf.aivory.id".into()),
            dmarc_report_address: env::var("DMARC_REPORT_ADDRESS").unwrap_or_else(|_| "dmarc@aivory.id".into()),
        }
    }

    pub fn is_cloudflare(&self) -> bool { self.mail_mode == "cloudflare" || self.mail_mode == "hybrid" }
    pub fn is_vps(&self) -> bool { self.mail_mode == "vps" || self.mail_mode == "hybrid" }
}
