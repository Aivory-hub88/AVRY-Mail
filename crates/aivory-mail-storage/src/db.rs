use sqlx::{PgPool, SqlitePool};
use anyhow::Result;

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
