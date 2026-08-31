use std::{sync::Arc, net::SocketAddr};
use aivory_mail_api::{config::Config, api, realtime::RealtimeHub};
use aivory_mail_storage::{db::DbPool, object_store::{ObjectStore, LocalStore}};
use tracing_subscriber::EnvFilter;
use tower_http::cors::{CorsLayer, Any};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("aivory_mail=debug".parse().unwrap()))
        .init();

    let config = Config::from_env();
    tracing::info!("Aivory Mail starting mode={} port={} db={} storage={}",
        config.mail_mode, config.port,
        if config.database_url.contains("postgres") {"postgres"} else {"sqlite"},
        config.storage_backend
    );

    // For sqlite, ensure parent dir and file exists
    let db_url = config.database_url.clone();
    if db_url.starts_with("sqlite://") && !db_url.contains("::memory:") {
        let path_part = db_url.strip_prefix("sqlite://").unwrap().split('?').next().unwrap();
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
        tracing::warn!("migration failed (may be first run): {}", e);
    }
    ensure_schema(&db).await.unwrap_or_else(|e| tracing::warn!("ensure_schema: {}", e));

    let store: Arc<dyn ObjectStore> = if config.storage_backend == "local" {
        let p = config.storage_path.clone();
        tokio::fs::create_dir_all(&p).await.ok();
        Arc::new(LocalStore::new(p))
    } else {
        tracing::warn!("S3/R2 backend requested but using local fallback for now");
        Arc::new(LocalStore::new(config.storage_path.clone()))
    };

    let hub = RealtimeHub::new();
    let state = Arc::new(api::AppState { config: config.clone(), db, store, hub });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = api::router(state)
        .layer(cors)
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
        "CREATE TABLE IF NOT EXISTS domains (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, domain TEXT UNIQUE NOT NULL, status TEXT NOT NULL DEFAULT 'Pending', dkim_selector TEXT NOT NULL DEFAULT 'aivory', sending_subdomain TEXT, cf_zone_id TEXT, created_at TEXT NOT NULL, verified_at TEXT)",
        "CREATE TABLE IF NOT EXISTS mailboxes (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, domain_id TEXT NOT NULL, address TEXT UNIQUE NOT NULL, display_name TEXT, is_catch_all INTEGER NOT NULL DEFAULT 0, forward_to TEXT, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS threads (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, mailbox_id TEXT NOT NULL, subject TEXT, participant_addrs TEXT NOT NULL DEFAULT '[]', message_count INTEGER NOT NULL DEFAULT 0, last_message_at TEXT NOT NULL, has_unread INTEGER NOT NULL DEFAULT 0)",
        "CREATE TABLE IF NOT EXISTS messages (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, mailbox_id TEXT NOT NULL, thread_id TEXT, message_id TEXT NOT NULL, from_addr TEXT NOT NULL DEFAULT '', from_name TEXT, to_addrs TEXT NOT NULL DEFAULT '[]', cc_addrs TEXT NOT NULL DEFAULT '[]', subject TEXT, snippet TEXT, body_text TEXT, body_html TEXT, folder TEXT NOT NULL DEFAULT 'Inbox', is_read INTEGER NOT NULL DEFAULT 0, is_starred INTEGER NOT NULL DEFAULT 0, raw_r2_key TEXT, size_bytes INTEGER NOT NULL DEFAULT 0, has_attachments INTEGER NOT NULL DEFAULT 0, headers_json TEXT, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS attachments (id TEXT PRIMARY KEY, message_id TEXT NOT NULL, filename TEXT NOT NULL, content_type TEXT NOT NULL, size_bytes INTEGER NOT NULL, r2_key TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS api_keys (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, name TEXT NOT NULL, key_hash TEXT NOT NULL, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS signatures (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL DEFAULT '', mailbox_id TEXT NOT NULL, name TEXT NOT NULL DEFAULT 'Default', html TEXT NOT NULL DEFAULT '', text TEXT NOT NULL DEFAULT '', is_default INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS calendar_proposals (id TEXT PRIMARY KEY, thread_id TEXT, message_id TEXT, event_type_slug TEXT, proposed_slots_json TEXT NOT NULL DEFAULT '[]', booking_url TEXT, status TEXT NOT NULL DEFAULT 'pending', created_at TEXT NOT NULL)",
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
    Ok(())
}
