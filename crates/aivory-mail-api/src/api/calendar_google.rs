//! Google Calendar connect/callback/disconnect + status, for Settings ->
//! Integrations -> Calendar. Mailbox identity always comes from the bearer
//! JWT (`authz::authenticated_email`), never from a client-supplied
//! `mailbox_id` — OAuth tokens are too sensitive for the trust-the-client
//! pattern `calendar_events.rs` uses for plain event CRUD.

use crate::api::AppState;
use crate::{auth, calendar_google, imap_password_vault};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::Redirect,
    Json,
};
use serde_json::Value;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

const OAUTH_STATE_ROLE: &str = "calendar_oauth_state";

async fn resolve_own_mailbox(state: &Arc<AppState>, headers: &HeaderMap) -> Result<String, StatusCode> {
    let email = crate::api::authz::authenticated_email(state, headers)?;
    resolve_mailbox_by_email(state, &email).await
}

async fn resolve_mailbox_by_email(state: &Arc<AppState>, email: &str) -> Result<String, StatusCode> {
    let row: Option<String> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("SELECT id FROM mailboxes WHERE lower(address)=lower($1) LIMIT 1")
                .bind(&email).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| r.get::<Uuid, _>("id").to_string())
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("SELECT id FROM mailboxes WHERE lower(address)=lower(?) LIMIT 1")
                .bind(&email).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| r.get::<String, _>("id"))
        }
    };
    row.ok_or(StatusCode::NOT_FOUND)
}

#[derive(serde::Deserialize)]
pub struct ConnectParams {
    /// The web app's `aivory_mail_token` — this is a full browser navigation
    /// (`window.location.href = ...`), not a `fetch`, so it cannot carry an
    /// Authorization header the way every other endpoint in this app does.
    token: String,
}

/// GET /v1/calendar/google/connect?token=... — redirects to Google's consent
/// screen. Public in `authz::require_user_mw`; auth happens here instead.
pub async fn connect(State(state): State<Arc<AppState>>, Query(params): Query<ConnectParams>) -> Result<Redirect, StatusCode> {
    let claims = auth::verify_jwt(&params.token, &state.config.jwt_secret).map_err(|_| StatusCode::UNAUTHORIZED)?;
    let email = claims.sub.trim().to_lowercase();
    let mailbox_id = resolve_mailbox_by_email(&state, &email).await?;
    let oauth_state = auth::sign_state_jwt(&state.config.jwt_secret, &mailbox_id, OAUTH_STATE_ROLE, 10)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let url = calendar_google::build_auth_url(&state, &oauth_state).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Redirect::temporary(&url))
}

#[derive(serde::Deserialize)]
pub struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// GET /v1/calendar/google/callback — exchanges the code, stores encrypted
/// tokens, redirects back into the web app's calendar settings.
pub async fn callback(State(state): State<Arc<AppState>>, Query(params): Query<CallbackParams>) -> Redirect {
    match callback_inner(&state, params).await {
        Ok(()) => Redirect::temporary("/settings/integrations?calendar=connected"),
        Err(msg) => {
            tracing::warn!("calendar_google callback failed: {}", msg);
            Redirect::temporary("/settings/integrations?calendar=error")
        }
    }
}

async fn callback_inner(state: &Arc<AppState>, params: CallbackParams) -> Result<(), String> {
    if let Some(err) = params.error {
        return Err(format!("google returned error: {err}"));
    }
    let code = params.code.ok_or("missing code")?;
    let oauth_state = params.state.ok_or("missing state")?;
    let claims = auth::verify_jwt(&oauth_state, &state.config.jwt_secret).map_err(|e| format!("invalid state: {e}"))?;
    if claims.role.as_deref() != Some(OAUTH_STATE_ROLE) {
        return Err("state token has wrong role".into());
    }
    let mailbox_id = claims.sub;

    let tokens = calendar_google::exchange_code(state, &code).await.map_err(|e| e.to_string())?;
    let access_token = tokens.get("access_token").and_then(|v| v.as_str()).ok_or("no access_token")?.to_string();
    let refresh_token = tokens.get("refresh_token").and_then(|v| v.as_str()).map(|s| s.to_string());
    let expires_in = tokens.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
    let token_expires_at = (chrono::Utc::now() + chrono::Duration::seconds(expires_in)).to_rfc3339();
    let google_email = calendar_google::fetch_google_email(&access_token).await.unwrap_or_default();

    // Google only returns a refresh_token on the *first* consent for this
    // client+user pair. Re-connecting without one keeps the previous
    // refresh_token (fetched below) instead of overwriting it with nothing.
    let account_id = Uuid::new_v4();
    let key = &state.config.imap_password_encryption_key;
    let access_encrypted = imap_password_vault::encrypt_oauth_token(key, account_id, &access_token).map_err(|_| "encrypt access_token failed")?;

    let existing_refresh_encrypted = existing_refresh_token(state, &mailbox_id).await;
    let refresh_encrypted = match (&refresh_token, &existing_refresh_encrypted) {
        (Some(rt), _) => imap_password_vault::encrypt_oauth_token(key, account_id, rt).map_err(|_| "encrypt refresh_token failed")?,
        (None, Some(existing)) => existing.clone(),
        (None, None) => return Err("google did not return a refresh_token and no prior one exists — reconnect with prompt=consent".into()),
    };

    upsert_account(state, account_id, &mailbox_id, &google_email, &access_encrypted, &refresh_encrypted, &token_expires_at).await
}

async fn existing_refresh_token(state: &Arc<AppState>, mailbox_id: &str) -> Option<String> {
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("SELECT refresh_token_encrypted FROM calendar_accounts WHERE mailbox_id=$1")
                .bind(mailbox_id).fetch_optional(pool).await.ok().flatten()
                .map(|r| r.get::<String, _>("refresh_token_encrypted"))
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("SELECT refresh_token_encrypted FROM calendar_accounts WHERE mailbox_id=?")
                .bind(mailbox_id).fetch_optional(pool).await.ok().flatten()
                .map(|r| r.get::<String, _>("refresh_token_encrypted"))
        }
    }
}

async fn upsert_account(
    state: &Arc<AppState>,
    id: Uuid,
    mailbox_id: &str,
    google_email: &str,
    access_encrypted: &str,
    refresh_encrypted: &str,
    token_expires_at: &str,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO calendar_accounts (id, mailbox_id, provider, google_account_email, access_token_encrypted, refresh_token_encrypted, token_expires_at, status, created_at, updated_at) \
                 VALUES ($1,$2,'google',$3,$4,$5,$6,'connected',$7,$7) \
                 ON CONFLICT (mailbox_id) DO UPDATE SET google_account_email=EXCLUDED.google_account_email, access_token_encrypted=EXCLUDED.access_token_encrypted, refresh_token_encrypted=EXCLUDED.refresh_token_encrypted, token_expires_at=EXCLUDED.token_expires_at, status='connected', last_error=NULL, updated_at=EXCLUDED.updated_at",
            )
            .bind(id).bind(mailbox_id).bind(google_email).bind(access_encrypted).bind(refresh_encrypted).bind(token_expires_at).bind(&now)
            .execute(pool).await.map_err(|e| e.to_string())?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO calendar_accounts (id, mailbox_id, provider, google_account_email, access_token_encrypted, refresh_token_encrypted, token_expires_at, status, created_at, updated_at) \
                 VALUES (?,?,'google',?,?,?,?,'connected',?,?) \
                 ON CONFLICT (mailbox_id) DO UPDATE SET google_account_email=excluded.google_account_email, access_token_encrypted=excluded.access_token_encrypted, refresh_token_encrypted=excluded.refresh_token_encrypted, token_expires_at=excluded.token_expires_at, status='connected', last_error=NULL, updated_at=excluded.updated_at",
            )
            .bind(id.to_string()).bind(mailbox_id).bind(google_email).bind(access_encrypted).bind(refresh_encrypted).bind(token_expires_at).bind(&now).bind(&now)
            .execute(pool).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// GET /v1/calendar/google — own mailbox's connection state.
pub async fn status(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = resolve_own_mailbox(&state, &headers).await?;
    let row: Option<(String, String, Option<String>, Option<String>)> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("SELECT google_account_email, status, last_synced_at, last_error FROM calendar_accounts WHERE mailbox_id=$1")
                .bind(&mailbox_id).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| (r.get("google_account_email"), r.get("status"), r.get("last_synced_at"), r.get("last_error")))
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("SELECT google_account_email, status, last_synced_at, last_error FROM calendar_accounts WHERE mailbox_id=?")
                .bind(&mailbox_id).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| (r.get("google_account_email"), r.get("status"), r.get("last_synced_at"), r.get("last_error")))
        }
    };
    match row {
        Some((email, status, last_synced_at, last_error)) => Ok(Json(serde_json::json!({
            "success": true,
            "data": {"connected": true, "google_account_email": email, "status": status, "last_synced_at": last_synced_at, "last_error": last_error}
        }))),
        None => Ok(Json(serde_json::json!({"success": true, "data": {"connected": false}}))),
    }
}

/// DELETE /v1/calendar/google — revoke + remove. Google-imported events for
/// this mailbox are deleted (cascade via calendar_event_links); events
/// created locally in Aivory are kept.
pub async fn disconnect(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = resolve_own_mailbox(&state, &headers).await?;
    let key = &state.config.imap_password_encryption_key;

    let row: Option<(String, String)> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("SELECT id, refresh_token_encrypted FROM calendar_accounts WHERE mailbox_id=$1")
                .bind(&mailbox_id).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| (r.get::<Uuid, _>("id").to_string(), r.get("refresh_token_encrypted")))
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("SELECT id, refresh_token_encrypted FROM calendar_accounts WHERE mailbox_id=?")
                .bind(&mailbox_id).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| (r.get("id"), r.get("refresh_token_encrypted")))
        }
    };
    let Some((account_id_str, refresh_encrypted)) = row else {
        return Ok(Json(serde_json::json!({"success": true, "data": {"connected": false}})));
    };
    if let Ok(account_id) = Uuid::parse_str(&account_id_str) {
        if let Ok(refresh_token) = imap_password_vault::decrypt_oauth_token(key, account_id, &refresh_encrypted) {
            let _ = calendar_google::revoke_token(&refresh_token).await;
        }
    }

    // Google-imported events for this mailbox go with the connection;
    // locally-created events (source='local') stay untouched.
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("DELETE FROM calendar_events WHERE mailbox_id=$1 AND source='google'").bind(&mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            sqlx::query("DELETE FROM calendar_accounts WHERE mailbox_id=$1").bind(&mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("DELETE FROM calendar_events WHERE mailbox_id=? AND source='google'").bind(&mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            sqlx::query("DELETE FROM calendar_accounts WHERE mailbox_id=?").bind(&mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(Json(serde_json::json!({"success": true, "data": {"connected": false}})))
}

/// POST /v1/calendar/google/sync-now — inline sync for the impatient-UX case
/// instead of waiting for the next 5-minute background tick.
pub async fn sync_now(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    let _ = resolve_own_mailbox(&state, &headers).await?;
    calendar_google::sync_all_accounts(&state).await;
    Ok(Json(serde_json::json!({"success": true})))
}
