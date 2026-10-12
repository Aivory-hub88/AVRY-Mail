//! A database built from nothing must reach the same state production has.
//!
//! Opt-in: set `AIVORY_TEST_POSTGRES_ADMIN_URL` to a server URL you may create
//! databases on (e.g. `postgresql://me@localhost:5432/postgres`). Each run uses
//! a throwaway database and drops it.

use aivory_mail_storage::db::DbPool;
use sqlx::{Connection, PgConnection, Row};

async fn scalar(pool: &sqlx::PgPool, sql: &str) -> String {
    sqlx::query(sql)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
        .try_get::<String, _>(0)
        .unwrap()
}

#[tokio::test]
async fn empty_postgres_migrates_to_the_production_shape() {
    let Ok(admin_url) = std::env::var("AIVORY_TEST_POSTGRES_ADMIN_URL") else {
        eprintln!("skipped: AIVORY_TEST_POSTGRES_ADMIN_URL is not set");
        return;
    };
    let name = format!("aivory_migrate_test_{}", uuid::Uuid::new_v4().simple());
    let mut admin = PgConnection::connect(&admin_url).await.expect("admin connection");
    sqlx::query(&format!("CREATE DATABASE {name}")).execute(&mut admin).await.expect("create database");

    let url = match admin_url.rsplit_once('/') {
        Some((base, _)) => format!("{base}/{name}"),
        None => panic!("unexpected admin url"),
    };
    // Run the checks in their own task so a failed assertion can't skip the
    // DROP below (a panic would otherwise unwind straight past it).
    let checks = tokio::spawn(async move {
        let db = DbPool::from_url(&url).await.expect("connect");
        db.migrate().await.expect("migrate an empty database");
        db.migrate().await.expect("migrating again is a no-op");
        let DbPool::Postgres(pool) = &db else { unreachable!() };

        // every migration applied, none failed
        let applied = scalar(pool, "SELECT COUNT(*) FILTER (WHERE success)::text FROM _sqlx_migrations").await;
        let on_disk = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations"))
            .unwrap()
            .filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "sql"))
            .count();
        assert_eq!(applied, on_disk.to_string());

        // the three tables migrations 002/017/024 could not create themselves
        for (table, column) in [("signatures", "mailbox_id"), ("email_integrations", "mailbox_id"), ("mailbox_aliases", "domain_id"), ("mailbox_aliases", "mailbox_id")] {
            let ty = scalar(pool, &format!("SELECT data_type FROM information_schema.columns WHERE table_name='{table}' AND column_name='{column}'")).await;
            assert_eq!(ty, "uuid", "{table}.{column}");
        }
        let fks = scalar(pool, "SELECT COUNT(*)::text FROM pg_constraint WHERE contype='f' AND conrelid::regclass::text IN ('signatures','email_integrations','mailbox_aliases')").await;
        assert_eq!(fks, "4", "foreign keys re-added by 026-028");

        // what the code is written against (migration 037)
        assert_eq!(scalar(pool, "SELECT COUNT(*)::text FROM tenants").await, "1", "default tenant");
        assert_eq!(scalar(pool, "SELECT data_type FROM information_schema.columns WHERE table_name='mailboxes' AND column_name='use_all_domains'").await, "integer");
        assert_eq!(scalar(pool, "SELECT COUNT(*)::text FROM information_schema.tables WHERE table_name IN ('groups','group_members')").await, "2");
        assert_eq!(scalar(pool, "SELECT COUNT(*)::text FROM information_schema.columns WHERE table_name='api_keys' AND column_name='key_raw'").await, "1");
    })
    .await;

    sqlx::query(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)")).execute(&mut admin).await.ok();
    if let Err(e) = checks {
        std::panic::resume_unwind(e.into_panic());
    }
}
