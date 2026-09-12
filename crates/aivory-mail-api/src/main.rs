use aivory_mail_api::{api, config::Config, realtime::RealtimeHub};
use aivory_mail_storage::{
    db::DbPool,
    object_store::{LocalStore, ObjectStore},
};
use axum::extract::DefaultBodyLimit;
use std::{net::SocketAddr, sync::Arc};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    limit::RequestBodyLimitLayer,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("aivory_mail=debug".parse().unwrap()),
        )
        .init();

    // Install rustls CryptoProvider for mail-send
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let config = Config::from_env();
    tracing::info!(
        "Aivory Mail starting mode={} port={} db={} storage={}",
        config.mail_mode,
        config.port,
        if config.database_url.contains("postgres") {
            "postgres"
        } else {
            "sqlite"
        },
        config.storage_backend
    );

    // For sqlite, ensure parent dir and file exists
    let db_url = config.database_url.clone();
    if db_url.starts_with("sqlite://") && !db_url.contains("::memory:") {
        let path_part = db_url
            .strip_prefix("sqlite://")
            .unwrap()
            .split('?')
            .next()
            .unwrap();
        if !path_part.is_empty() && path_part != ":memory:" {
            let p = std::path::Path::new(path_part);
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            // touch file
            if !p.exists() {
                let _ = std::fs::File::create(p);
            }
        }
    }

    let db = DbPool::from_url(&db_url).await?;
    if let Err(e) = db.migrate().await {
        if db_url.starts_with("postgres") {
            return Err(anyhow::anyhow!(
                "database migration failed; refusing to start: {}",
                e
            ));
        }
        tracing::warn!("migration failed for non-Postgres database: {}", e);
    }
    if matches!(&db, DbPool::Sqlite(_)) {
        ensure_schema(&db).await?;
    }

    let store: Arc<dyn ObjectStore> = if config.storage_backend == "local" {
        let p = config.storage_path.clone();
        tokio::fs::create_dir_all(&p).await.ok();
        Arc::new(LocalStore::new(p))
    } else {
        tracing::warn!("S3/R2 backend requested but using local fallback for now");
        Arc::new(LocalStore::new(config.storage_path.clone()))
    };

    let hub = RealtimeHub::new();
    let state = Arc::new(api::AppState {
        config: config.clone(),
        db,
        store,
        hub,
    });

    let cors = if config.cors_origins.iter().any(|o| o == "*") {
        CorsLayer::permissive()
    } else {
        let origins: Vec<_> = config
            .cors_origins
            .iter()
            .filter_map(|o| o.parse::<axum::http::HeaderValue>().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PUT,
                axum::http::Method::DELETE,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers([
                axum::http::header::CONTENT_TYPE,
                axum::http::header::AUTHORIZATION,
                axum::http::header::HeaderName::from_static("x-internal-token"),
            ])
            .allow_credentials(false)
    };

    let app = api::router(state)
        .layer(cors)
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(100 * 1024 * 1024))
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn ensure_schema(db: &DbPool) -> anyhow::Result<()> {
    let stmts = vec![
        "CREATE TABLE IF NOT EXISTS tenants (id TEXT PRIMARY KEY, slug TEXT UNIQUE NOT NULL, name TEXT NOT NULL, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS domains (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, domain TEXT UNIQUE NOT NULL, status TEXT NOT NULL DEFAULT 'Pending', dkim_selector TEXT NOT NULL DEFAULT 'aivory', sending_subdomain TEXT, cf_zone_id TEXT, created_at TEXT NOT NULL, verified_at TEXT, verification_token TEXT, dkim_public_key TEXT, dkim_private_key TEXT, failure_reason TEXT, admin_email TEXT)",
        "CREATE TABLE IF NOT EXISTS mailboxes (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, domain_id TEXT NOT NULL, address TEXT UNIQUE NOT NULL, display_name TEXT, is_catch_all INTEGER NOT NULL DEFAULT 0, use_all_domains INTEGER NOT NULL DEFAULT 0, forward_to TEXT, password_hash TEXT, password_hash_dovecot TEXT, imap_password_encrypted TEXT, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS threads (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, mailbox_id TEXT NOT NULL, subject TEXT, participant_addrs TEXT NOT NULL DEFAULT '[]', message_count INTEGER NOT NULL DEFAULT 0, last_message_at TEXT NOT NULL, has_unread INTEGER NOT NULL DEFAULT 0)",
        "CREATE TABLE IF NOT EXISTS messages (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, mailbox_id TEXT NOT NULL, thread_id TEXT, message_id TEXT NOT NULL, from_addr TEXT NOT NULL DEFAULT '', from_name TEXT, to_addrs TEXT NOT NULL DEFAULT '[]', cc_addrs TEXT NOT NULL DEFAULT '[]', subject TEXT, snippet TEXT, body_text TEXT, body_html TEXT, folder TEXT NOT NULL DEFAULT 'Inbox', is_read INTEGER NOT NULL DEFAULT 0, is_starred INTEGER NOT NULL DEFAULT 0, snoozed_until TEXT, raw_r2_key TEXT, size_bytes INTEGER NOT NULL DEFAULT 0, has_attachments INTEGER NOT NULL DEFAULT 0, headers_json TEXT, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS attachments (id TEXT PRIMARY KEY, message_id TEXT NOT NULL, filename TEXT NOT NULL, content_type TEXT NOT NULL, size_bytes INTEGER NOT NULL, r2_key TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS api_keys (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, name TEXT NOT NULL, key_hash TEXT NOT NULL, key_raw TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS mcp_capability_grants (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, mailbox_id TEXT NOT NULL, caller_id TEXT NOT NULL, audience TEXT NOT NULL, scopes TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE, jti TEXT NOT NULL UNIQUE, expires_at TEXT NOT NULL, revoked_at TEXT, last_used_at TEXT, created_at TEXT NOT NULL)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_grants_tenant_mailbox_revoked ON mcp_capability_grants(tenant_id, mailbox_id, revoked_at)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_grants_expiry_revoked ON mcp_capability_grants(expires_at, revoked_at)",
        "CREATE TABLE IF NOT EXISTS mcp_send_confirmations (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, mailbox_id TEXT NOT NULL, caller_id TEXT NOT NULL, action TEXT NOT NULL, payload_hash TEXT NOT NULL, expires_at TEXT NOT NULL, issued_at TEXT NOT NULL, used_at TEXT)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_send_confirmations_scope ON mcp_send_confirmations(tenant_id, mailbox_id, caller_id, action, used_at)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_send_confirmations_expiry ON mcp_send_confirmations(expires_at, used_at)",
        "CREATE TABLE IF NOT EXISTS signatures (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL DEFAULT '', mailbox_id TEXT NOT NULL, name TEXT NOT NULL DEFAULT 'Default', html TEXT NOT NULL DEFAULT '', text TEXT NOT NULL DEFAULT '', is_default INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS calendar_proposals (id TEXT PRIMARY KEY, thread_id TEXT, message_id TEXT, event_type_slug TEXT, proposed_slots_json TEXT NOT NULL DEFAULT '[]', booking_url TEXT, status TEXT NOT NULL DEFAULT 'pending', created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS calendar_events (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL DEFAULT 'default', mailbox_id TEXT NOT NULL DEFAULT '', calendar TEXT NOT NULL DEFAULT 'Daemon Larkin', title TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', start_at TEXT NOT NULL, end_at TEXT NOT NULL, guests TEXT NOT NULL DEFAULT '[]', color TEXT NOT NULL DEFAULT 'blue', recurring TEXT NOT NULL DEFAULT 'never', notifications TEXT NOT NULL DEFAULT '10m', location TEXT NOT NULL DEFAULT '', conferencing TEXT NOT NULL DEFAULT 'none', conferencing_link TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS knowledge_cache (tenant_id TEXT NOT NULL, scope TEXT NOT NULL, compiled_json TEXT NOT NULL, cursor TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY (tenant_id, scope))",
        "CREATE TABLE IF NOT EXISTS user_settings (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL DEFAULT 'default', mailbox_id TEXT, category TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, updated_at TEXT NOT NULL, UNIQUE(tenant_id, mailbox_id, category, key))",
        "CREATE TABLE IF NOT EXISTS mail_filters (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL DEFAULT 'default', name TEXT NOT NULL, criteria_json TEXT NOT NULL DEFAULT '{}', action_json TEXT NOT NULL DEFAULT '{}', enabled INTEGER NOT NULL DEFAULT 1, priority INTEGER NOT NULL DEFAULT 0, scope TEXT NOT NULL DEFAULT 'mailbox', created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS mail_labels (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL DEFAULT 'default', name TEXT NOT NULL, color TEXT NOT NULL DEFAULT '#3b82f6', created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS webhooks (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL DEFAULT 'default', url TEXT NOT NULL, events TEXT NOT NULL DEFAULT '[\"email.received\"]', secret TEXT NOT NULL DEFAULT '', enabled INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS webhook_deliveries (id TEXT PRIMARY KEY, webhook_id TEXT NOT NULL, event TEXT NOT NULL, payload TEXT NOT NULL DEFAULT '{}', status TEXT NOT NULL DEFAULT 'pending', attempts INTEGER NOT NULL DEFAULT 0, last_error TEXT, next_retry_at TEXT, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS agent_tasks (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL DEFAULT 'default', mailbox_id TEXT, thread_id TEXT, message_id TEXT, type TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'needs_reply', title TEXT NOT NULL, body TEXT NOT NULL DEFAULT '', payload TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS vacation_responders (id TEXT PRIMARY KEY, mailbox_id TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 0, subject TEXT NOT NULL DEFAULT '', body TEXT NOT NULL DEFAULT '', start_at TEXT, end_at TEXT, interval_days INTEGER NOT NULL DEFAULT 1, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS vacation_deliveries (id TEXT PRIMARY KEY, mailbox_id TEXT NOT NULL, recipient TEXT NOT NULL, sent_at TEXT NOT NULL, UNIQUE(mailbox_id, recipient))",
        "CREATE TABLE IF NOT EXISTS vacation_replies_sent (mailbox_id TEXT NOT NULL, sender_addr TEXT NOT NULL, sent_at TEXT NOT NULL, PRIMARY KEY (mailbox_id, sender_addr))",
        "CREATE TABLE IF NOT EXISTS contacts (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL DEFAULT 'default', email TEXT NOT NULL, display_name TEXT NOT NULL DEFAULT '', blocked INTEGER NOT NULL DEFAULT 0, last_seen_at TEXT NOT NULL, created_at TEXT NOT NULL, UNIQUE(tenant_id, email))",
        "CREATE TABLE IF NOT EXISTS folders (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL DEFAULT 'default', mailbox_id TEXT NOT NULL, name TEXT NOT NULL, color TEXT NOT NULL DEFAULT '#006355', created_at TEXT NOT NULL, UNIQUE(mailbox_id, name))",
        "CREATE TABLE IF NOT EXISTS audit_logs (id TEXT PRIMARY KEY, actor_id TEXT, target_id TEXT, mailbox_id TEXT, message_id TEXT, action TEXT NOT NULL, metadata TEXT, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS send_as_aliases (id TEXT PRIMARY KEY, mailbox_id TEXT NOT NULL, alias_email TEXT NOT NULL, display_name TEXT NOT NULL DEFAULT '', is_default INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS message_labels (message_id TEXT NOT NULL, label_id TEXT NOT NULL, PRIMARY KEY (message_id, label_id))",
        "CREATE TABLE IF NOT EXISTS groups (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL DEFAULT 'default', name TEXT NOT NULL, email TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL, UNIQUE(tenant_id, email))",
        "CREATE TABLE IF NOT EXISTS group_members (group_id TEXT NOT NULL, mailbox_id TEXT NOT NULL, PRIMARY KEY (group_id, mailbox_id))",
        "CREATE TABLE IF NOT EXISTS ai_chat_history (id TEXT PRIMARY KEY, mailbox_id TEXT, user_email TEXT NOT NULL DEFAULT '', question TEXT NOT NULL, answer TEXT NOT NULL, context_json TEXT NOT NULL DEFAULT '{}', model TEXT NOT NULL DEFAULT 'heuristic', created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS mission_control_notifications (id TEXT PRIMARY KEY, type TEXT NOT NULL DEFAULT 'email_assistant', title TEXT NOT NULL, body TEXT NOT NULL, action_url TEXT, metadata_json TEXT NOT NULL DEFAULT '{}', is_read INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS mailbox_aliases (id TEXT PRIMARY KEY, domain_id TEXT NOT NULL, mailbox_id TEXT NOT NULL, local_part TEXT NOT NULL, created_at TEXT NOT NULL, UNIQUE(domain_id, local_part))",
        "CREATE TABLE IF NOT EXISTS email_integrations (id TEXT PRIMARY KEY, mailbox_id TEXT NOT NULL UNIQUE, host TEXT NOT NULL, port INTEGER NOT NULL, username TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'connected', last_tested_at TEXT, last_connected_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
    ];
    let alters = vec![
        "ALTER TABLE api_keys ADD COLUMN key_raw TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE messages ADD COLUMN snoozed_until TEXT",
        "ALTER TABLE calendar_events ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default'",
        "ALTER TABLE calendar_events ADD COLUMN mailbox_id TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE domains ADD COLUMN verification_token TEXT",
        "ALTER TABLE domains ADD COLUMN dkim_public_key TEXT",
        "ALTER TABLE domains ADD COLUMN dkim_private_key TEXT",
        "ALTER TABLE domains ADD COLUMN failure_reason TEXT",
        "ALTER TABLE mail_filters ADD COLUMN priority INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE mailboxes ADD COLUMN password_hash TEXT",
        "ALTER TABLE mail_filters ADD COLUMN scope TEXT NOT NULL DEFAULT 'mailbox'",
        "ALTER TABLE mail_labels ADD COLUMN mailbox_id TEXT",
        "ALTER TABLE mail_filters ADD COLUMN mailbox_id TEXT",
        "ALTER TABLE contacts ADD COLUMN mailbox_id TEXT",
        "ALTER TABLE mailboxes ADD COLUMN use_all_domains INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE domains ADD COLUMN admin_email TEXT",
        "ALTER TABLE mailboxes ADD COLUMN password_hash_dovecot TEXT",
        "ALTER TABLE mailboxes ADD COLUMN imap_password_encrypted TEXT",
        "ALTER TABLE messages ADD COLUMN maildir_file TEXT",
    ];
    for sql in stmts {
        match db {
            DbPool::Postgres(pool) => {
                let _ = sqlx::query(sql).execute(pool).await;
            }
            DbPool::Sqlite(pool) => {
                sqlx::query(sql).execute(pool).await?;
            }
        }
    }
    for sql in alters {
        match db {
            DbPool::Postgres(pool) => {
                let _ = sqlx::query(sql).execute(pool).await;
            }
            DbPool::Sqlite(pool) => {
                let _ = sqlx::query(sql).execute(pool).await;
            }
        }
    }
    if let DbPool::Sqlite(pool) = db {
        // The original SQLite contacts table has UNIQUE(tenant_id, email),
        // which prevents independent records for the same sender in two
        // mailboxes. Rebuild it once with mailbox-aware uniqueness; the
        // marker index makes this idempotent on later startups.
        let scoped_index: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_contacts_tenant_mailbox_email'",
        )
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        if scoped_index == 0 {
            sqlx::query("DROP TABLE IF EXISTS contacts_scoped")
                .execute(pool)
                .await?;
            sqlx::query("CREATE TABLE contacts_scoped (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL DEFAULT 'default', mailbox_id TEXT, email TEXT NOT NULL, display_name TEXT NOT NULL DEFAULT '', blocked INTEGER NOT NULL DEFAULT 0, last_seen_at TEXT NOT NULL, created_at TEXT NOT NULL, UNIQUE(tenant_id, mailbox_id, email))")
                .execute(pool)
                .await?;
            sqlx::query("INSERT INTO contacts_scoped (id, tenant_id, mailbox_id, email, display_name, blocked, last_seen_at, created_at) SELECT id, tenant_id, mailbox_id, email, display_name, blocked, last_seen_at, created_at FROM contacts")
                .execute(pool)
                .await?;
            sqlx::query("DROP TABLE contacts").execute(pool).await?;
            sqlx::query("ALTER TABLE contacts_scoped RENAME TO contacts")
                .execute(pool)
                .await?;
            sqlx::query("CREATE INDEX IF NOT EXISTS idx_contacts_tenant_email ON contacts(tenant_id, email)")
                .execute(pool)
                .await?;
            sqlx::query("CREATE INDEX IF NOT EXISTS idx_contacts_tenant_mailbox_email ON contacts(tenant_id, mailbox_id, email)")
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}
