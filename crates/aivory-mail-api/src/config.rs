use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::env;

#[derive(Clone)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub storage_backend: String, // local | r2 | s3
    pub storage_bucket: String,
    pub storage_path: String,
    pub jwt_secret: String,
    pub internal_token: String,
    /// Deployment-managed AES-256-GCM key. It is intentionally not printable.
    pub imap_password_encryption_key: [u8; 32],
    pub mail_mode: String, // cloudflare | vps | hybrid
    pub cf_api_token: Option<String>,
    pub cf_zone_id: Option<String>,
    pub cf_account_id: Option<String>,
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
    pub worker_send_url: Option<String>,
    /// Origin this API is reachable at from a browser — used to rewrite
    /// `cid:` inline-image references in received HTML bodies into real,
    /// fetchable attachment URLs (browsers can't resolve `cid:` at all).
    pub public_base_url: String,
    /// Google OAuth client credentials for the Calendar sync feature.
    /// `None` when unset — calendar connect endpoints return a clear error
    /// instead of the whole process refusing to start, since this is an
    /// optional per-deployment feature, not core mail functionality.
    pub google_oauth_client_id: Option<String>,
    pub google_oauth_client_secret: Option<String>,
    pub google_oauth_redirect_url: String,
    /// Web Push (new-mail notifications with no tab open). `None` when
    /// WEB_PUSH_VAPID_PRIVATE_KEY is unset: the feature reports itself off.
    pub web_push: Option<crate::webpush::Vapid>,
}

/// Production unless a developer explicitly says otherwise.
///
/// This used to be the reverse: production only when RUST_ENV/ENV/NODE_ENV
/// said "production". The prod compose sets none of those on avry-mail, so
/// every production guard below (admin credentials, CORS wildcard,
/// INSPECTION_MODE, the aivory.uk DNS defaults) was silently off in prod.
/// Now local development opts out with RUST_ENV=development (or
/// AIVORY_MAIL_ENV); forgetting it fails loudly instead of quietly
/// weakening prod.
pub fn is_production(lookup: impl Fn(&str) -> Option<String>) -> bool {
    const DEV: &[&str] = &["development", "dev", "local", "test"];
    !["AIVORY_MAIL_ENV", "RUST_ENV"].iter().any(|key| {
        lookup(key)
            .map(|v| DEV.contains(&v.trim().to_ascii_lowercase().as_str()))
            .unwrap_or(false)
    })
}

impl Config {
    pub fn from_env() -> Self {
        let is_prod = is_production(|key| env::var(key).ok());
        // Fail-closed in every build, not only when a prod flag happens to
        // be set (see aivory_mail_core::secrets).
        let jwt_secret = aivory_mail_core::secrets::require_env_secret("JWT_SECRET");
        let internal_token = aivory_mail_core::secrets::require_env_secret("INTERNAL_TOKEN");
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            if is_prod {
                eprintln!("[FATAL] DATABASE_URL must be set in production");
                std::process::exit(1);
            }
            "sqlite::memory:".into()
        });
        let imap_password_encryption_key = env::var("IMAP_PASSWORD_ENCRYPTION_KEY")
            .ok()
            .and_then(|value| STANDARD.decode(value).ok())
            .and_then(|bytes| bytes.try_into().ok())
            .unwrap_or_else(|| {
                eprintln!("[FATAL] IMAP_PASSWORD_ENCRYPTION_KEY must be standard base64 for exactly 32 bytes");
                std::process::exit(1);
            });
        // Blank counts as missing: compose turns an unset .env entry into "".
        let non_blank = |key: &str| env::var(key).ok().filter(|v| !v.trim().is_empty());
        let mail_admin_email = non_blank("MAIL_ADMIN_EMAIL").unwrap_or_else(|| {
            if is_prod {
                eprintln!("[FATAL] MAIL_ADMIN_EMAIL must be set in production");
                std::process::exit(1);
            }
            "admin@localhost".into()
        });
        // This password signs the admin into every admin route, so in prod it
        // gets the same check as the other secrets (no blank, no placeholder).
        let mail_admin_password = if is_prod {
            aivory_mail_core::secrets::require_env_secret("MAIL_ADMIN_PASSWORD")
        } else {
            non_blank("MAIL_ADMIN_PASSWORD").unwrap_or_else(|| "change-me-in-development".into())
        };
        let cors_value = non_blank("CORS_ORIGINS").unwrap_or_else(|| {
        if is_prod { eprintln!("[FATAL] CORS_ORIGINS must be set in production"); std::process::exit(1); }
        "http://localhost:3005,http://localhost:3000,http://localhost:9000,http://localhost:9001".into()
    });
        if is_prod && cors_value.split(',').any(|origin| origin.trim() == "*") {
            eprintln!("[FATAL] wildcard CORS is forbidden in production");
            std::process::exit(1);
        }
        // A key that is set but broken is a deploy mistake: say so at start
        // instead of silently sending nothing.
        let web_push = crate::webpush::Vapid::from_env().unwrap_or_else(|e| {
            eprintln!("[FATAL] {e}");
            std::process::exit(1);
        });
        if is_prod
            && env::var("INSPECTION_MODE")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false)
        {
            eprintln!("[FATAL] INSPECTION_MODE must be disabled in production");
            std::process::exit(1);
        }
        Self {
            port: env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8095),
            database_url,
            storage_backend: env::var("STORAGE_BACKEND").unwrap_or_else(|_| "local".into()),
            storage_bucket: env::var("STORAGE_BUCKET").unwrap_or_else(|_| "aivory-mail".into()),
            storage_path: env::var("STORAGE_PATH").unwrap_or_else(|_| "./data/mail-storage".into()),
            jwt_secret,
            internal_token,
            imap_password_encryption_key,
            mail_mode: env::var("MAIL_MODE").unwrap_or_else(|_| "vps".into()),
            cf_api_token: env::var("CF_API_TOKEN").ok(),
            cf_zone_id: env::var("CF_ZONE_ID").ok(),
            cf_account_id: env::var("CF_ACCOUNT_ID").ok(),
            smtp_host: env::var("SMTP_HOST").ok(),
            smtp_port: env::var("SMTP_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(587),
            ai_gateway_url: env::var("AI_GATEWAY_URL")
                .or_else(|_| env::var("ZEROCLAW_URL"))
                .ok(),
            workflow_url: env::var("WORKFLOW_URL")
                .or_else(|_| env::var("N8N_AS_CODE_URL"))
                .ok(),
            cors_origins: cors_value
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
            r2_endpoint: env::var("R2_ENDPOINT").ok(),
            r2_access_key: env::var("R2_ACCESS_KEY_ID").ok(),
            r2_secret_key: env::var("R2_SECRET_ACCESS_KEY").ok(),
            mail_intelligence_model: env::var("MAIL_INTELLIGENCE_MODEL")
                .unwrap_or_else(|_| "deepseek/deepseek-v4-flash-0731".into()),
            diagnostic_model: env::var("DIAGNOSTIC_MODEL")
                .unwrap_or_else(|_| "qwen/qwen3-235b-a22b".into()),
            cognee_url: env::var("COGNEE_URL")
                .or_else(|_| env::var("COGNEE_CERVEAU_URL"))
                .ok(),
            cognee_secret: env::var("COGNEE_INTERNAL_SECRET")
                .or_else(|_| env::var("X_CERVEAU_INTERNAL_SECRET"))
                .ok(),
            cognee_agent_type: env::var("COGNEE_AGENT_TYPE").unwrap_or_else(|_| "mail_ops".into()),
            mail_admin_email,
            mail_admin_password,
            mail_mx_host: env::var("MAIL_MX_HOST").unwrap_or_else(|_| {
                if is_prod {
                    "mail.aivory.uk".into()
                } else {
                    "mail.aivory.id".into()
                }
            }),
            spf_include_host: env::var("SPF_INCLUDE_HOST").unwrap_or_else(|_| {
                if is_prod {
                    "_spf.aivory.uk".into()
                } else {
                    "_spf.aivory.id".into()
                }
            }),
            dmarc_report_address: env::var("DMARC_REPORT_ADDRESS").unwrap_or_else(|_| {
                if is_prod {
                    "dmarc@aivory.uk".into()
                } else {
                    "dmarc@aivory.id".into()
                }
            }),
            worker_send_url: env::var("WORKER_SEND_URL")
                .ok()
                .or_else(|| env::var("CF_WORKER_SEND_URL").ok()),
            public_base_url: env::var("PUBLIC_API_URL").unwrap_or_else(|_| {
                if is_prod {
                    "https://mail.aivory.uk".into()
                } else {
                    "http://localhost:8095".into()
                }
            }),
            google_oauth_client_id: env::var("GOOGLE_OAUTH_CLIENT_ID").ok(),
            google_oauth_client_secret: env::var("GOOGLE_OAUTH_CLIENT_SECRET").ok(),
            google_oauth_redirect_url: env::var("GOOGLE_OAUTH_REDIRECT_URL").unwrap_or_else(|_| {
                format!(
                    "{}/v1/calendar/google/callback",
                    env::var("PUBLIC_API_URL").unwrap_or_else(|_| if is_prod {
                        "https://mail.aivory.uk".into()
                    } else {
                        "http://localhost:8095".into()
                    })
                )
            }),
            web_push,
        }
    }

    pub fn for_tests(database_url: &str) -> Self {
        Self {
            port: 0,
            database_url: database_url.to_string(),
            storage_backend: "local".to_string(),
            storage_bucket: "aivory-mail-test".to_string(),
            storage_path: std::env::temp_dir()
                .join("aivory-mail-test-storage")
                .to_string_lossy()
                .into_owned(),
            jwt_secret: "phase2-test-jwt-secret".to_string(),
            internal_token: "test-internal-token".to_string(),
            imap_password_encryption_key: [0u8; 32],
            mail_mode: "vps".to_string(),
            cf_api_token: None,
            cf_zone_id: None,
            cf_account_id: None,
            smtp_host: None,
            smtp_port: 587,
            ai_gateway_url: None,
            workflow_url: None,
            cors_origins: vec!["http://localhost".to_string()],
            r2_endpoint: None,
            r2_access_key: None,
            r2_secret_key: None,
            mail_intelligence_model: "test-model".to_string(),
            diagnostic_model: "test-model".to_string(),
            cognee_url: None,
            cognee_secret: Some("test-cerveau-secret".to_string()),
            cognee_agent_type: "test-agent".to_string(),
            mail_admin_email: "admin@test.local".to_string(),
            mail_admin_password: "test-admin-password".to_string(),
            mail_mx_host: "mail.test.local".to_string(),
            spf_include_host: "_spf.test.local".to_string(),
            dmarc_report_address: "dmarc@test.local".to_string(),
            worker_send_url: None,
            public_base_url: "http://localhost".to_string(),
            google_oauth_client_id: None,
            google_oauth_client_secret: None,
            google_oauth_redirect_url: "http://localhost/v1/calendar/google/callback".to_string(),
            web_push: None,
        }
    }

    pub fn is_cloudflare(&self) -> bool {
        self.mail_mode == "cloudflare" || self.mail_mode == "hybrid"
    }
    pub fn is_vps(&self) -> bool {
        self.mail_mode == "vps" || self.mail_mode == "hybrid"
    }
}

#[cfg(test)]
mod production_default_tests {
    use super::is_production;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string())
    }

    #[test]
    fn no_flags_means_production() {
        // The prod compose sets none of these on avry-mail.
        assert!(is_production(env(&[])));
        assert!(is_production(env(&[("NODE_ENV", "development")]))); // not ours to read
    }

    #[test]
    fn development_is_an_explicit_opt_out() {
        assert!(!is_production(env(&[("RUST_ENV", "development")])));
        assert!(!is_production(env(&[("AIVORY_MAIL_ENV", "Local ")])));
        assert!(!is_production(env(&[("RUST_ENV", "test")])));
    }

    #[test]
    fn anything_else_stays_production() {
        assert!(is_production(env(&[("RUST_ENV", "production")])));
        assert!(is_production(env(&[("RUST_ENV", "staging")])));
        assert!(is_production(env(&[("RUST_ENV", "")])));
    }
}
