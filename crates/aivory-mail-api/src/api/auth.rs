use crate::api::AppState;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    email: String,
    exp: usize,
    iat: usize,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<Value>, StatusCode> {
    let email = body.email.trim().to_lowercase();
    let password = body.password.trim().to_string();

    if email.is_empty() || password.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Check against config admin credentials (env-driven, production fail-closed handled in config.rs)
    let admin_email = state.config.mail_admin_email.to_lowercase();
    let admin_password = state.config.mail_admin_password.clone();

    // Also check if email is a mailbox address in DB — if found, allow password = admin_password or mailbox-specific?
    // For production, also allow any mailbox with password = admin_password (simple shared password for MVP)
    // In future, integrate Supabase or proper user table.

    let is_admin_match = email == admin_email && password == admin_password;

    // Check if mailbox exists and, if it has its own password set (via admin
    // console "Create account" password field), verify against that hash —
    // a mailbox with a real password should NOT also accept the shared
    // MAIL_ADMIN_PASSWORD.
    let (_mailbox_exists, mailbox_password_hash): (bool, Option<String>) = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            match sqlx::query("SELECT password_hash FROM mailboxes WHERE lower(address)=$1")
                .bind(&email)
                .fetch_optional(pool)
                .await
                .unwrap_or(None)
            {
                Some(row) => (true, row.get::<Option<String>, _>("password_hash")),
                None => (false, None),
            }
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            match sqlx::query("SELECT password_hash FROM mailboxes WHERE lower(address)=?")
                .bind(&email)
                .fetch_optional(pool)
                .await
                .unwrap_or(None)
            {
                Some(row) => (true, row.get::<Option<String>, _>("password_hash")),
                None => (false, None),
            }
        }
    };
    let has_own_password = mailbox_password_hash.is_some();
    let own_password_match = mailbox_password_hash
        .as_deref()
        .map(|h| aivory_mail_core::password::verify_password(&password, h))
        .unwrap_or(false);

    let superadmin_emails: Vec<String> = std::env::var("SUPERADMIN_EMAIL")
        .unwrap_or_default()
        .split(',')
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect();
    let is_superadmin = superadmin_emails
        .iter()
        .any(|candidate| candidate == &email)
        && password == admin_password;
    // Mailbox accounts must authenticate with their own password. The shared
    // admin credential is reserved for the explicitly configured admin and
    // optional superadmin identities; inspection mode cannot create a login
    // bypass for arbitrary mailbox addresses.
    let allowed = own_password_match || (!has_own_password && (is_admin_match || is_superadmin));

    if !allowed {
        return Ok(Json(
            serde_json::json!({"success": false, "error": "Invalid email or password"}),
        ));
    }

    let now = chrono::Utc::now().timestamp() as usize;
    let exp = (chrono::Utc::now() + chrono::Duration::days(7)).timestamp() as usize;
    let claims = Claims {
        sub: email.clone(),
        email: email.clone(),
        exp,
        iat: now,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.jwt_secret.as_bytes()),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(
        serde_json::json!({"success": true, "data": {"token": token, "email": email, "expires_at": exp}}),
    ))
}

/// Resolve the logged-in mailbox from the bearer JWT. The web app calls this
/// right after login (and on load) to learn which mailbox_id it is — without
/// it, every "Inbox"/"Sent"/"Spam"/"Trash" list request omitted mailbox_id
/// entirely and the API fell back to returning messages across *all*
/// mailboxes, which is why folders looked mixed between accounts.
pub async fn me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    let email = crate::api::authz::authenticated_email(&state, &headers)?;

    let mailbox: Option<(String, String, Option<String>)> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("SELECT id, address, display_name FROM mailboxes WHERE lower(address)=$1")
                .bind(&email)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| {
                    (
                        r.get::<uuid::Uuid, _>("id").to_string(),
                        r.get("address"),
                        r.get("display_name"),
                    )
                })
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("SELECT id, address, display_name FROM mailboxes WHERE lower(address)=?")
                .bind(&email)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| (r.get("id"), r.get("address"), r.get("display_name")))
        }
    };

    let is_admin = crate::api::authz::is_admin(&state, &email).await;

    let data_json = match mailbox {
        Some((id, address, display_name)) => serde_json::json!({
            "email": email, "mailbox_id": id, "address": address, "display_name": display_name, "is_admin": is_admin,
        }),
        None => {
            serde_json::json!({ "email": email, "mailbox_id": null, "address": null, "display_name": null, "is_admin": is_admin })
        }
    };
    Ok(Json(
        serde_json::json!({"success": true, "data": data_json}),
    ))
}
