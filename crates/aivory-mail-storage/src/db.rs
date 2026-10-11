use sqlx::{PgPool, SqlitePool};
use anyhow::Result;

/// Migrations 002, 017 and 024 declare `signatures.mailbox_id`,
/// `email_integrations.mailbox_id` and `mailbox_aliases.{domain_id,mailbox_id}`
/// as TEXT with a foreign key to a UUID column, which Postgres rejects
/// ("foreign key constraint ... cannot be implemented"). Production applied
/// them back when it still worked, and 026-028 later normalized the columns to
/// UUID, so those three files can't be edited (sqlx checksums them) and an
/// empty database can't get past 002.
///
/// 026-028 were written for this: "the bootstrap in DbPool pre-creates a
/// UUID-compatible table for fresh or partially migrated databases so
/// migration 002 remains checksum-stable and its IF NOT EXISTS is harmless".
/// That bootstrap was never committed. These are those tables, in the shape
/// 026-028 create when they find the table missing. 026-028 then add the
/// foreign keys and indexes.
const LEGACY_TABLES: [&str; 3] = [
    "CREATE TABLE IF NOT EXISTS signatures (
        id UUID PRIMARY KEY,
        tenant_id TEXT NOT NULL DEFAULT '',
        mailbox_id UUID NOT NULL,
        name TEXT NOT NULL DEFAULT 'Default',
        html TEXT NOT NULL DEFAULT '',
        text TEXT NOT NULL DEFAULT '',
        is_default BOOLEAN NOT NULL DEFAULT FALSE,
        created_at TEXT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS email_integrations (
        id UUID PRIMARY KEY,
        mailbox_id UUID NOT NULL UNIQUE,
        host TEXT NOT NULL,
        port INTEGER NOT NULL,
        username TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'connected',
        last_tested_at TEXT,
        last_connected_at TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS mailbox_aliases (
        id UUID PRIMARY KEY,
        domain_id UUID NOT NULL,
        mailbox_id UUID NOT NULL,
        local_part TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE(domain_id, local_part)
    )",
];

/// First migration that normalizes the legacy tables. A database that has
/// applied it (production) never needs the bootstrap.
const NORMALIZED_FROM_VERSION: i64 = 26;

async fn bootstrap_legacy_tables(pool: &PgPool) -> Result<()> {
    let ledger: Option<String> = sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations')::text")
        .fetch_one(pool)
        .await?;
    if ledger.is_some() {
        let applied: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success")
            .fetch_one(pool)
            .await?;
        if applied.unwrap_or(0) >= NORMALIZED_FROM_VERSION {
            return Ok(());
        }
    }
    for statement in LEGACY_TABLES {
        sqlx::query(statement).execute(pool).await?;
    }
    Ok(())
}

#[derive(Clone)]
pub enum DbPool {
    Postgres(PgPool),
    Sqlite(SqlitePool),
}

impl DbPool {
    pub async fn from_url(url: &str) -> Result<Self> {
        if url.starts_with("postgres") {
            let pool = PgPool::connect(url).await?;
            Ok(Self::Postgres(pool))
        } else {
            // sqlite: sqlite::memory: or sqlite://path
            let pool = SqlitePool::connect(url).await?;
            Ok(Self::Sqlite(pool))
        }
    }

    pub fn is_postgres(&self) -> bool { matches!(self, Self::Postgres(_)) }

    pub async fn migrate(&self) -> Result<()> {
        match self {
            Self::Postgres(pool) => {
                bootstrap_legacy_tables(pool).await?;
                sqlx::migrate!("../../migrations").run(pool).await?;
            }
            Self::Sqlite(pool) => {
                // SQLite uses ensure_schema in main.rs; migrations are Postgres-only
                // Try migrate but ignore if fails (Postgres syntax not SQLite compatible)
                if let Err(e) = sqlx::migrate!("../../migrations").run(pool).await {
                    tracing::warn!("sqlite migrate skipped (expected): {}", e);
                }
            }
        }
        Ok(())
    }

    pub async fn health_check(&self) -> Result<()> {
        match self {
            Self::Postgres(p) => { sqlx::query("SELECT 1").execute(p).await?; Ok(()) }
            Self::Sqlite(p) => { sqlx::query("SELECT 1").execute(p).await?; Ok(()) }
        }
    }
}
