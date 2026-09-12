//! Maildir mirror for Dovecot IMAP.
//!
//! Postgres stays the system of record (threads, labels, AI, search). This
//! module additionally delivers every inbound/sent message into a Maildir
//! tree (`MAILDIR_PATH`, default `/data/maildir`) laid out exactly like
//! Dovecot expects (`<domain>/<user>/{cur,new,tmp}` plus Maildir++
//! subfolders like `.Sent`), so a stock Dovecot serves the same mail over
//! IMAP with zero protocol work on our side. `\Seen` is synced back from
//! `is_read` (filename `,S` flag plus `new/`→`cur/` move).
//!
//! Everything here is best-effort: a failed write/rename is logged and
//! ignored, it never fails delivery or flag updates.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

fn base_path() -> PathBuf {
    std::env::var("MAILDIR_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/data/maildir"))
}

fn split_address(address: &str) -> Option<(String, String)> {
    let (user, domain) = address.split_once('@')?;
    if user.is_empty() || domain.is_empty() {
        return None;
    }
    Some((user.to_lowercase(), domain.to_lowercase()))
}

/// Maildir++ subdir for an app folder. `None` = INBOX (top-level cur/new/tmp).
fn subdir_for_folder(folder: &str) -> Option<String> {
    match folder {
        "Inbox" => None,
        "Sent" => Some(".Sent".to_string()),
        "Drafts" => Some(".Drafts".to_string()),
        "Spam" => Some(".Junk".to_string()),
        "Trash" => Some(".Trash".to_string()),
        "Archive" => Some(".Archive".to_string()),
        other => {
            let clean: String = other
                .chars()
                .filter(|c| {
                    c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.'
                })
                .take(48)
                .collect();
            if clean.is_empty() {
                Some(".Other".to_string())
            } else {
                Some(format!(".{clean}"))
            }
        }
    }
}

async fn append_subscription(home: &Path, sub: &str) {
    let path = home.join("subscriptions");
    let current = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    if current.lines().any(|l| l.trim() == sub) {
        return;
    }
    let mut out = current;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(sub);
    out.push('\n');
    let _ = tokio::fs::write(&path, out).await;
}

fn unique_base(size: usize) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let rand = Uuid::new_v4().simple().to_string();
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "aivory".into());
    format!(
        "{nanos}.M{pid}P{rand}.{host},S={size}",
        pid = std::process::id()
    )
}

/// Deliver raw RFC5322 bytes into the Maildir tree. Returns the stored
/// relative path (`domain/user/[.Sub/]{cur|new}/filename`) for `maildir_file`.
pub async fn deliver_raw(
    address: &str,
    folder: &str,
    seen: bool,
    raw: &[u8],
) -> anyhow::Result<String> {
    let (user, domain) = split_address(address).ok_or_else(|| anyhow::anyhow!("bad address"))?;
    let home = base_path().join(&domain).join(&user);
    let sub = subdir_for_folder(folder);
    let dir: PathBuf = match &sub {
        None => home.clone(),
        Some(s) => home.join(s),
    };
    for d in ["cur", "new", "tmp"] {
        tokio::fs::create_dir_all(dir.join(d)).await?;
    }
    if let Some(s) = &sub {
        append_subscription(&home, s).await;
    }
    let uniq = unique_base(raw.len());
    let (target, fname) = if seen {
        ("cur", format!("{uniq}:2,S"))
    } else {
        ("new", uniq)
    };
    let tmp_path = dir.join("tmp").join(format!("{fname}.tmp"));
    tokio::fs::write(&tmp_path, raw).await?;
    tokio::fs::rename(&tmp_path, dir.join(target).join(&fname)).await?;
    let rel = match &sub {
        None => format!("{domain}/{user}/{target}/{fname}"),
        Some(s) => format!("{domain}/{user}/{s}/{target}/{fname}"),
    };
    Ok(rel)
}

/// Resolve a mailbox address for Maildir placement.
pub async fn mailbox_address(state: &Arc<AppState>, mailbox_id: &Uuid) -> Option<String> {
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query_scalar::<_, String>("SELECT address FROM mailboxes WHERE id=$1")
                .bind(mailbox_id)
                .fetch_optional(pool)
                .await
                .unwrap_or(None)
        }
        DbPool::Sqlite(pool) => {
            sqlx::query_scalar::<_, String>("SELECT address FROM mailboxes WHERE id=?")
                .bind(mailbox_id.to_string())
                .fetch_optional(pool)
                .await
                .unwrap_or(None)
        }
    }
}

/// Persist the Maildir relative path on the message row.
pub async fn record_maildir_file(
    state: &Arc<AppState>,
    id: &Uuid,
    relpath: &str,
) -> anyhow::Result<()> {
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE messages SET maildir_file=$1 WHERE id=$2")
                .bind(relpath)
                .bind(id)
                .execute(pool)
                .await?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE messages SET maildir_file=? WHERE id=?")
                .bind(relpath)
                .bind(id.to_string())
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

/// Flip the `\Seen` flag to match `is_read`: renames the file to add/remove
/// the `S` flag and moves between `new/` and `cur/` per Maildir convention.
/// Returns the new relative path when the file moved/renamed.
pub async fn set_seen(relative: &str, seen: bool) -> anyhow::Result<Option<String>> {
    let full = base_path().join(relative);
    let parent = full
        .parent()
        .ok_or_else(|| anyhow::anyhow!("bad maildir path"))?;
    let name = full
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("bad maildir name"))?;
    let subdir = parent
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if subdir == "tmp" {
        anyhow::bail!("transient tmp file, nothing to sync");
    }
    let _grandparent = parent.parent().ok_or_else(|| anyhow::anyhow!("bad maildir path"))?;

    // Split "base:2,FLAGS" (files in new/ usually carry no :2, suffix).
    let (base, mut flags): (String, Vec<char>) = match name.split_once(":2,") {
        Some((b, f)) => (b.to_string(), f.chars().collect()),
        None => (name.to_string(), Vec::new()),
    };
    let has_s = flags.contains(&'S');
    if has_s == seen && ((seen && subdir == "cur") || (!seen && subdir == "new")) {
        return Ok(None);
    }

    if seen {
        if !has_s {
            flags.push('S');
            flags.sort_unstable();
        }
    } else {
        flags.retain(|c| *c != 'S');
    }
    let flag_str: String = flags.into_iter().collect();

    // Relative path of the grandparent dir (domain/user[/.Sub]) for rebuild.
    let rel_parent = Path::new(relative)
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| anyhow::anyhow!("bad maildir path"))?;

    let (new_subdir, new_name) = if seen {
        ("cur", format!("{base}:2,{flag_str}"))
    } else {
        ("new", base)
    };
    let new_rel = rel_parent.join(new_subdir).join(&new_name);
    let new_full = base_path().join(&new_rel);
    tokio::fs::rename(&full, &new_full).await?;
    Ok(Some(new_rel.to_string_lossy().into_owned()))
}
