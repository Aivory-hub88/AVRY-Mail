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
    pub mail_admin_email: String,
    pub mail_admin_password: String,
    pub mail_mx_host: String,
    /// Hostname referenced by the SPF `include:` mechanism in customer SPF records —
    /// Aivory's own domain publishes the actual sending-IP TXT record there.
    pub spf_include_host: String,
    pub dmarc_report_address: String,
}

impl Config {
    pub fn from_env() -> Self {
        let is_prod = env::var("RUST_ENV").map(|v| v=="production").unwrap_or(false)
            || env::var("ENV").map(|v| v=="production").unwrap_or(false)
            || env::var("NODE_ENV").map(|v| v=="production").unwrap_or(false);
        let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| {
            if is_prod { eprintln!("[FATAL] JWT_SECRET must be set in production"); std::process::exit(1); }
            "aivory-mail-dev-secret-change-me".into()
        });
        let internal_token = env::var("INTERNAL_TOKEN").unwrap_or_else(|_| {
            if is_prod { eprintln!("[FATAL] INTERNAL_TOKEN must be set in production"); std::process::exit(1); }
            "aivory-internal-dev".into()
        });
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            if is_prod { eprintln!("[FATAL] DATABASE_URL must be set in production"); std::process::exit(1); }
            "sqlite::memory:".into()
        });
        if is_prod && jwt_secret=="aivory-mail-dev-secret-change-me" { eprintln!("[FATAL] JWT_SECRET is default dev value in production"); std::process::exit(1); }
        if is_prod && internal_token=="aivory-internal-dev" { eprintln!("[FATAL] INTERNAL_TOKEN is default dev value in production"); std::process::exit(1); }
        Self {
            port: env::var("PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(8095),
            database_url,
            storage_backend: env::var("STORAGE_BACKEND").unwrap_or_else(|_| "local".into()),
            storage_bucket: env::var("STORAGE_BUCKET").unwrap_or_else(|_| "aivory-mail".into()),
            storage_path: env::var("STORAGE_PATH").unwrap_or_else(|_| "./data/mail-storage".into()),
            jwt_secret,
            internal_token,
            mail_mode: env::var("MAIL_MODE").unwrap_or_else(|_| "vps".into()),
            cf_api_token: env::var("CF_API_TOKEN").ok(),
            cf_zone_id: env::var("CF_ZONE_ID").ok(),
            smtp_host: env::var("SMTP_HOST").ok(),
            smtp_port: env::var("SMTP_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(587),
            ai_gateway_url: env::var("AI_GATEWAY_URL").or_else(|_| env::var("ZEROCLAW_URL")).ok(),
            workflow_url: env::var("WORKFLOW_URL").or_else(|_| env::var("N8N_AS_CODE_URL")).ok(),
            cors_origins: env::var("CORS_ORIGINS").unwrap_or_else(|_| {
                if is_prod { "https://aivory.id,https://mail.aivory.uk".into() } else { "http://localhost:3005,http://localhost:3000,http://localhost:9000,http://localhost:9001".into() }
            }).split(',').map(|s| s.trim().to_string()).collect(),
            r2_endpoint: env::var("R2_ENDPOINT").ok(),
            r2_access_key: env::var("R2_ACCESS_KEY_ID").ok(),
            r2_secret_key: env::var("R2_SECRET_ACCESS_KEY").ok(),
            mail_intelligence_model: env::var("MAIL_INTELLIGENCE_MODEL").unwrap_or_else(|_| "deepseek/deepseek-v4-flash-0731".into()),
            diagnostic_model: env::var("DIAGNOSTIC_MODEL").unwrap_or_else(|_| "qwen/qwen3-235b-a22b".into()),
            cognee_url: env::var("COGNEE_URL").or_else(|_| env::var("COGNEE_CERVEAU_URL")).ok(),
            cognee_secret: env::var("COGNEE_INTERNAL_SECRET").or_else(|_| env::var("X_CERVEAU_INTERNAL_SECRET")).ok(),
            cognee_agent_type: env::var("COGNEE_AGENT_TYPE").unwrap_or_else(|_| "mail_ops".into()),
            mail_admin_email: env::var("MAIL_ADMIN_EMAIL").unwrap_or_else(|_| "admin@aivory.id".into()),
            mail_admin_password: env::var("MAIL_ADMIN_PASSWORD").unwrap_or_else(|_| "Avry786876!@".into()),
            mail_mx_host: env::var("MAIL_MX_HOST").unwrap_or_else(|_| if is_prod { "mail.aivory.uk".into() } else { "mail.aivory.id".into() }),
            spf_include_host: env::var("SPF_INCLUDE_HOST").unwrap_or_else(|_| if is_prod { "_spf.aivory.uk".into() } else { "_spf.aivory.id".into() }),
            dmarc_report_address: env::var("DMARC_REPORT_ADDRESS").unwrap_or_else(|_| if is_prod { "dmarc@aivory.uk".into() } else { "dmarc@aivory.id".into() }),
        }
    }

    pub fn is_cloudflare(&self) -> bool { self.mail_mode == "cloudflare" || self.mail_mode == "hybrid" }
    pub fn is_vps(&self) -> bool { self.mail_mode == "vps" || self.mail_mode == "hybrid" }
}
