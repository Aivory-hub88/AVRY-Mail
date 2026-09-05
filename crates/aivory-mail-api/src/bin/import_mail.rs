//! One-off .eml backlog importer (Zoho/other-provider export -> Aivory Mail).
//!
//! The export folder layout (as produced by most providers' "export mailbox"
//! tools) is: <base_dir>/<mailbox-folder-name>/<Folder>/<id>.eml — e.g.
//!   Email migrations/irfan-reichmann/Inbox/12345.eml
//!   Email migrations/hello-aivory/Notification/67890.eml
//!
//! Usage:
//!   DATABASE_URL=postgresql://... cargo run --bin import_mail -- \
//!     "/path/to/Email migrations" \
//!     irfan-reichmann=irfan.reichmann@aivory.uk \
//!     hello-aivory=hello@aivory.uk
//!
//! Each `<dir-name>=<mailbox-address>` argument maps one export subfolder to
//! the mailbox it should land in. The target mailbox must already exist
//! (create it in Admin -> Accounts first) OR be reachable via a send-as
//! alias — `hello@aivory.uk` failing to resolve here was the original "alias
//! not imported" bug: inbound resolution only checked the mailboxes table,
//! never send_as_aliases (fixed in mail/routing.rs).
//!
//! Folder mapping: Inbox/Sent/Drafts pass straight through as system
//! folders. Anything else (Notification, Newsletter, ...) is imported as a
//! custom folder of the same name (created if missing) rather than dumped
//! into Inbox, since those aren't part of the Inbox/Sent/Drafts/Spam/Trash
//! set the UI treats as system folders.
//!
//! Re-running is safe: import_message() skips a .eml whose Message-ID is
//! already present in the target mailbox.

use aivory_mail_api::{config::Config, api::AppState, mail::inbound::import_message, realtime::RealtimeHub};
use aivory_mail_storage::{db::DbPool, object_store::{ObjectStore, LocalStore}};
use std::{collections::HashMap, path::Path, sync::Arc};
use uuid::Uuid;
use sqlx::Row;

const SYSTEM_FOLDERS: &[&str] = &["Inbox", "Sent", "Drafts", "Spam", "Trash", "Archive"];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: import_mail <base_dir> <export-subfolder>=<mailbox-address> [more mappings...]");
        std::process::exit(1);
    }
    let base_dir = Path::new(&args[0]);
    let mut mapping: HashMap<String, String> = HashMap::new();
    for arg in &args[1..] {
        let Some((k, v)) = arg.split_once('=') else {
            eprintln!("bad mapping '{}', expected <export-subfolder>=<mailbox-address>", arg);
            std::process::exit(1);
        };
        mapping.insert(k.to_string(), v.to_lowercase());
    }

    let config = Config::from_env();
    let db = DbPool::from_url(&config.database_url).await?;
    let store: Arc<dyn ObjectStore> = Arc::new(LocalStore::new(config.storage_path.clone()));
    let state = Arc::new(AppState { config: config.clone(), db, store, hub: RealtimeHub::new() });

    let mut total_imported = 0u32;
    let mut total_skipped = 0u32;
    let mut total_failed = 0u32;

    for (export_dir, address) in &mapping {
        let mailbox_dir = base_dir.join(export_dir);
        if !mailbox_dir.is_dir() {
            eprintln!("skip: {} not found under {}", export_dir, base_dir.display());
            continue;
        }
        let Some((mailbox_id, tenant_id)) = resolve_mailbox(&state, address).await? else {
            eprintln!(
                "SKIP MAILBOX {} ({}): no mailbox row and no send-as alias found — create the account in Admin -> Accounts (or Admin -> Aliases) first",
                export_dir, address
            );
            continue;
        };
        println!("== {} -> {} ({})", export_dir, address, mailbox_id);

        let mut folder_entries: Vec<_> = std::fs::read_dir(&mailbox_dir)?.filter_map(|e| e.ok()).collect();
        folder_entries.sort_by_key(|e| e.file_name());
        for entry in folder_entries {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let export_folder_name = entry.file_name().to_string_lossy().to_string();
            let target_folder = if SYSTEM_FOLDERS.contains(&export_folder_name.as_str()) {
                export_folder_name.clone()
            } else {
                ensure_custom_folder(&state, &mailbox_id, &export_folder_name).await?;
                export_folder_name.clone()
            };

            let mut emls: Vec<_> = std::fs::read_dir(&path)?.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "eml").unwrap_or(false))
                .collect();
            emls.sort_by_key(|e| e.file_name());

            for eml in emls {
                let raw = match std::fs::read(eml.path()) {
                    Ok(r) => r,
                    Err(e) => { eprintln!("  read failed {}: {}", eml.path().display(), e); total_failed += 1; continue; }
                };
                match import_message(&state, &tenant_id, &mailbox_id, &target_folder, raw).await {
                    Ok(Some(_)) => total_imported += 1,
                    Ok(None) => total_skipped += 1,
                    Err(e) => { eprintln!("  import failed {}: {}", eml.path().display(), e); total_failed += 1; }
                }
            }
            println!("  {}: done", target_folder);
        }
    }

    println!("\nimported={} already_present={} failed={}", total_imported, total_skipped, total_failed);
    Ok(())
}

async fn resolve_mailbox(state: &Arc<AppState>, address: &str) -> anyhow::Result<Option<(Uuid, Uuid)>> {
    match &state.db {
        DbPool::Postgres(pool) => {
            if let Some(row) = sqlx::query("SELECT id, tenant_id FROM mailboxes WHERE lower(address)=$1")
                .bind(address).fetch_optional(pool).await?
            {
                return Ok(Some((row.get("id"), row.get("tenant_id"))));
            }
            if let Some(row) = sqlx::query(
                "SELECT m.id, m.tenant_id FROM send_as_aliases a JOIN mailboxes m ON m.id = a.mailbox_id WHERE lower(a.alias_email)=$1"
            ).bind(address).fetch_optional(pool).await?
            {
                return Ok(Some((row.get("id"), row.get("tenant_id"))));
            }
            Ok(None)
        }
        DbPool::Sqlite(pool) => {
            if let Some(row) = sqlx::query("SELECT id, tenant_id FROM mailboxes WHERE lower(address)=?")
                .bind(address).fetch_optional(pool).await?
            {
                let id: String = row.get("id");
                let tid: String = row.get("tenant_id");
                return Ok(Some((Uuid::parse_str(&id)?, Uuid::parse_str(&tid)?)));
            }
            if let Some(row) = sqlx::query(
                "SELECT m.id, m.tenant_id FROM send_as_aliases a JOIN mailboxes m ON m.id = a.mailbox_id WHERE lower(a.alias_email)=?"
            ).bind(address).fetch_optional(pool).await?
            {
                let id: String = row.get("id");
                let tid: String = row.get("tenant_id");
                return Ok(Some((Uuid::parse_str(&id)?, Uuid::parse_str(&tid)?)));
            }
            Ok(None)
        }
    }
}

async fn ensure_custom_folder(state: &Arc<AppState>, mailbox_id: &Uuid, name: &str) -> anyhow::Result<()> {
    let id = Uuid::new_v4();
    // Best-effort: folders(mailbox_id, name) is unique in the SQLite schema
    // but the Postgres migration text doesn't guarantee the same index name
    // exists on every deployed DB, so swallow the conflict instead of
    // depending on ON CONFLICT matching a specific constraint.
    match &state.db {
        DbPool::Postgres(pool) => {
            let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM folders WHERE mailbox_id=$1 AND name=$2")
                .bind(mailbox_id).bind(name).fetch_one(pool).await.unwrap_or(0);
            if exists == 0 {
                let _ = sqlx::query("INSERT INTO folders (id, tenant_id, mailbox_id, name, created_at) VALUES ($1,'default',$2,$3,NOW())")
                    .bind(id).bind(mailbox_id).bind(name).execute(pool).await;
            }
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT OR IGNORE INTO folders (id, tenant_id, mailbox_id, name, color, created_at) VALUES (?,'default',?,?,'#006355',?)")
                .bind(id.to_string()).bind(mailbox_id.to_string()).bind(name).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await?;
        }
    }
    Ok(())
}
