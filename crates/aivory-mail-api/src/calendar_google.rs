//! Google Calendar OAuth + sync engine. Mirrors the shape of `calendar.rs`
//! (the Calnode client) — plain reqwest calls returning `anyhow::Result<Value>`,
//! no heavy OAuth/HTTP framework. Two pieces:
//!   - token exchange/refresh helpers used by `api::calendar_google` (the
//!     connect/callback/disconnect HTTP handlers)
//!   - `sync_all_accounts`, run on a `tokio::spawn` interval from `main.rs`,
//!     that pulls Google events into `calendar_events` and pushes
//!     locally-created events out to Google.

use crate::api::AppState;
use crate::imap_password_vault;
use anyhow::{anyhow, Result};
use chrono::{Duration, Utc};
use serde_json::Value;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const SCOPE: &str = "https://www.googleapis.com/auth/calendar";

pub fn build_auth_url(state: &Arc<AppState>, oauth_state: &str) -> Option<String> {
    let client_id = state.config.google_oauth_client_id.as_ref()?;
    Some(format!(
        "{AUTH_URL}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&state={}",
        urlencoding::encode(client_id),
        urlencoding::encode(&state.config.google_oauth_redirect_url),
        urlencoding::encode(SCOPE),
        urlencoding::encode(oauth_state),
    ))
}

/// Exchange an authorization `code` for an access+refresh token pair.
pub async fn exchange_code(state: &Arc<AppState>, code: &str) -> Result<Value> {
    let client_id = state
        .config
        .google_oauth_client_id
        .as_ref()
        .ok_or_else(|| anyhow!("Google Calendar integration not configured"))?;
    let client_secret = state
        .config
        .google_oauth_client_secret
        .as_ref()
        .ok_or_else(|| anyhow!("Google Calendar integration not configured"))?;
    let r = reqwest::Client::new()
        .post(TOKEN_URL)
        .form(&[
            ("code", code),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", state.config.google_oauth_redirect_url.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await?;
    if !r.status().is_success() {
        let txt = r.text().await.unwrap_or_default();
        anyhow::bail!("google token exchange failed: {}", txt);
    }
    Ok(r.json().await?)
}

async fn refresh_access_token(state: &Arc<AppState>, refresh_token: &str) -> Result<Value> {
    let client_id = state
        .config
        .google_oauth_client_id
        .as_ref()
        .ok_or_else(|| anyhow!("Google Calendar integration not configured"))?;
    let client_secret = state
        .config
        .google_oauth_client_secret
        .as_ref()
        .ok_or_else(|| anyhow!("Google Calendar integration not configured"))?;
    let r = reqwest::Client::new()
        .post(TOKEN_URL)
        .form(&[
            ("refresh_token", refresh_token),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await?;
    if !r.status().is_success() {
        let txt = r.text().await.unwrap_or_default();
        anyhow::bail!("google token refresh failed: {}", txt);
    }
    Ok(r.json().await?)
}

pub async fn revoke_token(token: &str) -> Result<()> {
    let _ = reqwest::Client::new()
        .post(REVOKE_URL)
        .form(&[("token", token)])
        .send()
        .await;
    Ok(())
}

pub async fn fetch_google_email(access_token: &str) -> Result<String> {
    let r = reqwest::Client::new()
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(access_token)
        .send()
        .await?;
    let v: Value = r.json().await?;
    v.get("email")
        .and_then(|e| e.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("google userinfo missing email"))
}

struct Account {
    id: Uuid,
    mailbox_id: String,
    access_token_encrypted: String,
    refresh_token_encrypted: String,
    token_expires_at: chrono::DateTime<Utc>,
    sync_token: Option<String>,
    calendar_id: String,
}

/// Runs one pass over every connected `calendar_accounts` row. Called from a
/// `tokio::time::interval` loop in `main.rs`; errors on one account never
/// stop the others — each account's failure is recorded on its own row.
pub async fn sync_all_accounts(state: &Arc<AppState>) {
    let accounts = match load_accounts(state).await {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!("calendar_google: failed to load accounts: {}", e);
            return;
        }
    };
    for account in accounts {
        if let Err(e) = sync_account(state, &account).await {
            tracing::warn!("calendar_google: sync failed for account {}: {}", account.id, e);
            let _ = mark_error(state, account.id, &e.to_string()).await;
        }
    }
}

async fn load_accounts(state: &Arc<AppState>) -> Result<Vec<Account>> {
    const SQL: &str = "SELECT id, mailbox_id, access_token_encrypted, refresh_token_encrypted, token_expires_at, sync_token, calendar_id FROM calendar_accounts WHERE status='connected'";
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            let rows = sqlx::query(SQL).fetch_all(pool).await?;
            Ok(rows
                .into_iter()
                .filter_map(|row| {
                    let id: Uuid = row.try_get("id").ok()?;
                    let expires_raw: String = row.try_get("token_expires_at").ok()?;
                    Some(Account {
                        id,
                        mailbox_id: row.try_get("mailbox_id").ok()?,
                        access_token_encrypted: row.try_get("access_token_encrypted").ok()?,
                        refresh_token_encrypted: row.try_get("refresh_token_encrypted").ok()?,
                        token_expires_at: chrono::DateTime::parse_from_rfc3339(&expires_raw)
                            .map(|d| d.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now() - Duration::hours(1)),
                        sync_token: row.try_get("sync_token").ok(),
                        calendar_id: row.try_get("calendar_id").ok().unwrap_or_else(|| "primary".into()),
                    })
                })
                .collect())
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let rows = sqlx::query(SQL).fetch_all(pool).await?;
            Ok(rows
                .into_iter()
                .filter_map(|row| {
                    let id_str: String = row.try_get("id").ok()?;
                    let id = Uuid::parse_str(&id_str).ok()?;
                    let expires_raw: String = row.try_get("token_expires_at").ok()?;
                    Some(Account {
                        id,
                        mailbox_id: row.try_get("mailbox_id").ok()?,
                        access_token_encrypted: row.try_get("access_token_encrypted").ok()?,
                        refresh_token_encrypted: row.try_get("refresh_token_encrypted").ok()?,
                        token_expires_at: chrono::DateTime::parse_from_rfc3339(&expires_raw)
                            .map(|d| d.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now() - Duration::hours(1)),
                        sync_token: row.try_get("sync_token").ok(),
                        calendar_id: row.try_get("calendar_id").ok().unwrap_or_else(|| "primary".into()),
                    })
                })
                .collect())
        }
    }
}

async fn valid_access_token(state: &Arc<AppState>, account: &Account) -> Result<String> {
    let key = &state.config.imap_password_encryption_key;
    if account.token_expires_at > Utc::now() + Duration::minutes(2) {
        return imap_password_vault::decrypt_oauth_token(key, account.id, &account.access_token_encrypted)
            .map_err(|_| anyhow!("failed to decrypt access token"));
    }
    let refresh_token = imap_password_vault::decrypt_oauth_token(key, account.id, &account.refresh_token_encrypted)
        .map_err(|_| anyhow!("failed to decrypt refresh token"))?;
    let refreshed = refresh_access_token(state, &refresh_token).await?;
    let access_token = refreshed
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("refresh response missing access_token"))?
        .to_string();
    let expires_in = refreshed.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
    let new_expiry = Utc::now() + Duration::seconds(expires_in);
    let encrypted = imap_password_vault::encrypt_oauth_token(key, account.id, &access_token)
        .map_err(|_| anyhow!("failed to encrypt refreshed access token"))?;
    persist_refreshed_token(state, account.id, &encrypted, new_expiry).await?;
    Ok(access_token)
}

async fn persist_refreshed_token(
    state: &Arc<AppState>,
    account_id: Uuid,
    access_token_encrypted: &str,
    expires_at: chrono::DateTime<Utc>,
) -> Result<()> {
    let expires_str = expires_at.to_rfc3339();
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("UPDATE calendar_accounts SET access_token_encrypted=$1, token_expires_at=$2, updated_at=NOW() WHERE id=$3")
                .bind(access_token_encrypted).bind(&expires_str).bind(account_id)
                .execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE calendar_accounts SET access_token_encrypted=?, token_expires_at=?, updated_at=? WHERE id=?")
                .bind(access_token_encrypted).bind(&expires_str).bind(chrono::Utc::now().to_rfc3339()).bind(account_id.to_string())
                .execute(pool).await?;
        }
    }
    Ok(())
}

async fn mark_error(state: &Arc<AppState>, account_id: Uuid, err: &str) -> Result<()> {
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("UPDATE calendar_accounts SET status='error', last_error=$1 WHERE id=$2")
                .bind(err).bind(account_id).execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE calendar_accounts SET status='error', last_error=? WHERE id=?")
                .bind(err).bind(account_id.to_string()).execute(pool).await?;
        }
    }
    Ok(())
}

async fn mark_synced(state: &Arc<AppState>, account_id: Uuid, next_sync_token: Option<&str>) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("UPDATE calendar_accounts SET status='connected', last_error=NULL, last_synced_at=$1, sync_token=COALESCE($2, sync_token) WHERE id=$3")
                .bind(&now).bind(next_sync_token).bind(account_id).execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE calendar_accounts SET status='connected', last_error=NULL, last_synced_at=?, sync_token=COALESCE(?, sync_token) WHERE id=?")
                .bind(&now).bind(next_sync_token).bind(account_id.to_string()).execute(pool).await?;
        }
    }
    Ok(())
}

async fn sync_account(state: &Arc<AppState>, account: &Account) -> Result<()> {
    let access_token = valid_access_token(state, account).await?;
    let next_sync_token = pull_google_events(state, account, &access_token).await?;
    push_local_events(state, account, &access_token).await?;
    mark_synced(state, account.id, next_sync_token.as_deref()).await
}

/// Pulls Google's changes since the stored `sync_token` (or, on first sync,
/// events in a bounded ±6 month window) into `calendar_events`. Recurring
/// events come pre-expanded via `singleEvents=true` — no RRULE parsing
/// needed on our side.
async fn pull_google_events(state: &Arc<AppState>, account: &Account, access_token: &str) -> Result<Option<String>> {
    let client = reqwest::Client::new();
    let base = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events",
        urlencoding::encode(&account.calendar_id)
    );
    let mut url = if let Some(token) = &account.sync_token {
        format!("{base}?syncToken={}&singleEvents=true", urlencoding::encode(token))
    } else {
        let time_min = (Utc::now() - Duration::days(180)).to_rfc3339();
        let time_max = (Utc::now() + Duration::days(180)).to_rfc3339();
        format!(
            "{base}?singleEvents=true&orderBy=startTime&timeMin={}&timeMax={}",
            urlencoding::encode(&time_min),
            urlencoding::encode(&time_max)
        )
    };
    let mut next_sync_token: Option<String> = None;
    loop {
        let r = client.get(&url).bearer_auth(access_token).send().await?;
        if r.status() == reqwest::StatusCode::GONE {
            // Expired sync token — Google requires a full resync.
            clear_sync_token(state, account.id).await?;
            return Ok(None);
        }
        if !r.status().is_success() {
            let txt = r.text().await.unwrap_or_default();
            anyhow::bail!("google events.list failed: {}", txt);
        }
        let body: Value = r.json().await?;
        for item in body.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
            upsert_from_google(state, account, &item).await?;
        }
        if let Some(token) = body.get("nextSyncToken").and_then(|v| v.as_str()) {
            next_sync_token = Some(token.to_string());
        }
        match body.get("nextPageToken").and_then(|v| v.as_str()) {
            Some(page) => {
                url = format!("{base}?pageToken={}", urlencoding::encode(page));
            }
            None => break,
        }
    }
    Ok(next_sync_token)
}

async fn clear_sync_token(state: &Arc<AppState>, account_id: Uuid) -> Result<()> {
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("UPDATE calendar_accounts SET sync_token=NULL WHERE id=$1").bind(account_id).execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE calendar_accounts SET sync_token=NULL WHERE id=?").bind(account_id.to_string()).execute(pool).await?;
        }
    }
    Ok(())
}

async fn find_link_by_google_id(state: &Arc<AppState>, account_id: Uuid, google_event_id: &str) -> Result<Option<String>> {
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT local_event_id FROM calendar_event_links WHERE calendar_account_id=$1 AND google_event_id=$2")
                .bind(account_id).bind(google_event_id).fetch_optional(pool).await?;
            Ok(row.map(|r| r.get::<String, _>("local_event_id")))
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT local_event_id FROM calendar_event_links WHERE calendar_account_id=? AND google_event_id=?")
                .bind(account_id.to_string()).bind(google_event_id).fetch_optional(pool).await?;
            Ok(row.map(|r| r.get::<String, _>("local_event_id")))
        }
    }
}

async fn upsert_from_google(state: &Arc<AppState>, account: &Account, item: &Value) -> Result<()> {
    let google_event_id = item.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    if google_event_id.is_empty() {
        return Ok(());
    }
    let existing_local_id = find_link_by_google_id(state, account.id, &google_event_id).await?;
    let cancelled = item.get("status").and_then(|v| v.as_str()) == Some("cancelled");
    if cancelled {
        if let Some(local_id) = existing_local_id {
            delete_local_event(state, &local_id).await?;
        }
        return Ok(());
    }

    let title = item.get("summary").and_then(|v| v.as_str()).unwrap_or("(no title)").to_string();
    let description = item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let location = item.get("location").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let start_at = item
        .pointer("/start/dateTime")
        .or_else(|| item.pointer("/start/date"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let end_at = item
        .pointer("/end/dateTime")
        .or_else(|| item.pointer("/end/date"))
        .and_then(|v| v.as_str())
        .unwrap_or(&start_at)
        .to_string();
    if start_at.is_empty() {
        return Ok(());
    }

    match existing_local_id {
        Some(local_id) => {
            update_local_event(state, &local_id, &title, &description, &start_at, &end_at, &location).await?;
        }
        None => {
            let local_id = Uuid::new_v4();
            insert_local_event(state, &local_id, &account.mailbox_id, &title, &description, &start_at, &end_at, &location).await?;
            insert_link(state, account.id, &local_id.to_string(), &google_event_id, "google").await?;
        }
    }
    Ok(())
}

async fn insert_local_event(
    state: &Arc<AppState>,
    id: &Uuid,
    mailbox_id: &str,
    title: &str,
    description: &str,
    start_at: &str,
    end_at: &str,
    location: &str,
) -> Result<()> {
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO calendar_events (id, tenant_id, mailbox_id, calendar, title, description, start_at, end_at, location, source, created_at) VALUES ($1,'default',$2,'Google Calendar',$3,$4,$5,$6,$7,'google',NOW())")
                .bind(id).bind(mailbox_id).bind(title).bind(description).bind(start_at).bind(end_at).bind(location)
                .execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO calendar_events (id, tenant_id, mailbox_id, calendar, title, description, start_at, end_at, location, source, created_at) VALUES (?,'default',?,'Google Calendar',?,?,?,?,?,'google',?)")
                .bind(id.to_string()).bind(mailbox_id).bind(title).bind(description).bind(start_at).bind(end_at).bind(location).bind(chrono::Utc::now().to_rfc3339())
                .execute(pool).await?;
        }
    }
    Ok(())
}

async fn update_local_event(
    state: &Arc<AppState>,
    local_id: &str,
    title: &str,
    description: &str,
    start_at: &str,
    end_at: &str,
    location: &str,
) -> Result<()> {
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("UPDATE calendar_events SET title=$1, description=$2, start_at=$3, end_at=$4, location=$5 WHERE id=$6")
                .bind(title).bind(description).bind(start_at).bind(end_at).bind(location).bind(Uuid::parse_str(local_id).unwrap_or_default())
                .execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE calendar_events SET title=?, description=?, start_at=?, end_at=?, location=? WHERE id=?")
                .bind(title).bind(description).bind(start_at).bind(end_at).bind(location).bind(local_id)
                .execute(pool).await?;
        }
    }
    Ok(())
}

async fn delete_local_event(state: &Arc<AppState>, local_id: &str) -> Result<()> {
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("DELETE FROM calendar_events WHERE id=$1").bind(Uuid::parse_str(local_id).unwrap_or_default()).execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("DELETE FROM calendar_events WHERE id=?").bind(local_id).execute(pool).await?;
        }
    }
    Ok(())
}

async fn insert_link(state: &Arc<AppState>, account_id: Uuid, local_event_id: &str, google_event_id: &str, origin: &str) -> Result<()> {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO calendar_event_links (id, calendar_account_id, local_event_id, google_event_id, origin, created_at) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (local_event_id) DO UPDATE SET google_event_id=EXCLUDED.google_event_id")
                .bind(id).bind(account_id).bind(local_event_id).bind(google_event_id).bind(origin).bind(&now)
                .execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("INSERT OR REPLACE INTO calendar_event_links (id, calendar_account_id, local_event_id, google_event_id, origin, created_at) VALUES (?,?,?,?,?,?)")
                .bind(id.to_string()).bind(account_id.to_string()).bind(local_event_id).bind(google_event_id).bind(origin).bind(&now)
                .execute(pool).await?;
        }
    }
    Ok(())
}

/// Pushes `calendar_events` rows that were created locally (`source='local'`)
/// for this mailbox and don't yet have a `calendar_event_links` row.
struct UnlinkedEvent {
    id: String,
    title: String,
    description: String,
    start_at: String,
    end_at: String,
    location: String,
}

async fn push_local_events(state: &Arc<AppState>, account: &Account, access_token: &str) -> Result<()> {
    let unlinked: Vec<UnlinkedEvent> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            let rows = sqlx::query("SELECT e.id, e.title, e.description, e.start_at, e.end_at, e.location FROM calendar_events e LEFT JOIN calendar_event_links l ON l.local_event_id::text = e.id::text WHERE e.mailbox_id=$1 AND e.source='local' AND l.id IS NULL")
                .bind(&account.mailbox_id).fetch_all(pool).await?;
            rows.into_iter().map(|row| UnlinkedEvent {
                id: row.try_get::<Uuid, _>("id").map(|u| u.to_string()).unwrap_or_default(),
                title: row.try_get("title").unwrap_or_default(),
                description: row.try_get("description").unwrap_or_default(),
                start_at: row.try_get("start_at").unwrap_or_default(),
                end_at: row.try_get("end_at").unwrap_or_default(),
                location: row.try_get("location").unwrap_or_default(),
            }).collect()
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let rows = sqlx::query("SELECT e.id, e.title, e.description, e.start_at, e.end_at, e.location FROM calendar_events e LEFT JOIN calendar_event_links l ON l.local_event_id = e.id WHERE e.mailbox_id=? AND e.source='local' AND l.id IS NULL")
                .bind(&account.mailbox_id).fetch_all(pool).await?;
            rows.into_iter().map(|row| UnlinkedEvent {
                id: row.try_get("id").unwrap_or_default(),
                title: row.try_get("title").unwrap_or_default(),
                description: row.try_get("description").unwrap_or_default(),
                start_at: row.try_get("start_at").unwrap_or_default(),
                end_at: row.try_get("end_at").unwrap_or_default(),
                location: row.try_get("location").unwrap_or_default(),
            }).collect()
        }
    };
    let client = reqwest::Client::new();
    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events",
        urlencoding::encode(&account.calendar_id)
    );
    for ev in unlinked {
        let payload = serde_json::json!({
            "summary": ev.title,
            "description": ev.description,
            "location": ev.location,
            "start": {"dateTime": ev.start_at},
            "end": {"dateTime": ev.end_at},
        });
        let r = client.post(&url).bearer_auth(access_token).json(&payload).send().await?;
        if !r.status().is_success() {
            let txt = r.text().await.unwrap_or_default();
            tracing::warn!("calendar_google: push event {} failed: {}", ev.id, txt);
            continue;
        }
        let created: Value = r.json().await?;
        if let Some(google_event_id) = created.get("id").and_then(|v| v.as_str()) {
            insert_link(state, account.id, &ev.id, google_event_id, "aivory").await?;
        }
    }
    Ok(())
}
