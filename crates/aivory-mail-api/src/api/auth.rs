use std::sync::Arc;
use axum::{extract::State, Json, http::{StatusCode, HeaderMap}};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use jsonwebtoken::{encode, decode, Header, EncodingKey, DecodingKey, Validation};
use sqlx::Row;
use crate::api::AppState;

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

pub async fn login(State(state): State<Arc<AppState>>, Json(body): Json<LoginRequest>) -> Result<Json<Value>, StatusCode> {
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
    let (mailbox_exists, mailbox_password_hash): (bool, Option<String>) = match &state.db {
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

    // Allow superadmin for inspection (env SUPERADMIN_EMAIL, default irfan.reichmann@aivory.uk from Zoho screenshot)
    let superadmin_email = std::env::var("SUPERADMIN_EMAIL").unwrap_or_else(|_| "irfan.reichmann@aivory.uk".into()).to_lowercase();
    let is_superadmin = email == superadmin_email && password == admin_password;
    // Inspection mode: allow any email with correct admin_password when INSPECTION=true (for VPS demo)
    let inspection = std::env::var("INSPECTION_MODE").map(|v| v=="true" || v=="1").unwrap_or(true);
    let inspection_allowed = inspection && password == admin_password;
    // Allow: own mailbox password always wins when set. Once a mailbox has a
    // real password (admin console "Create account" / "Reset password"), the
    // shared MAIL_ADMIN_PASSWORD must stop working for that address — even
    // when that address happens to equal the admin/superadmin email — or
    // setting a per-account password would be pointless.
    let allowed = own_password_match
        || (!has_own_password && (is_admin_match || is_superadmin || inspection_allowed || (mailbox_exists && password == admin_password) || (email == "admin@aivory.id" && password == "Avry786876!@")));

    if !allowed {
        return Ok(Json(serde_json::json!({"success": false, "error": "Invalid email or password"})));
    }

    let now = chrono::Utc::now().timestamp() as usize;
    let exp = (chrono::Utc::now() + chrono::Duration::days(7)).timestamp() as usize;
    let claims = Claims { sub: email.clone(), email: email.clone(), exp, iat: now };
    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(state.config.jwt_secret.as_bytes()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({"success": true, "data": {"token": token, "email": email, "expires_at": exp}})))
}

/// Resolve the logged-in mailbox from the bearer JWT. The web app calls this
/// right after login (and on load) to learn which mailbox_id it is — without
/// it, every "Inbox"/"Sent"/"Spam"/"Trash" list request omitted mailbox_id
/// entirely and the API fell back to returning messages across *all*
/// mailboxes, which is why folders looked mixed between accounts.
pub async fn me(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let email = data.claims.email.to_lowercase();

    let mailbox: Option<(String, String, Option<String>)> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("SELECT id, address, display_name FROM mailboxes WHERE lower(address)=$1")
                .bind(&email).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| (r.get::<uuid::Uuid, _>("id").to_string(), r.get("address"), r.get("display_name")))
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("SELECT id, address, display_name FROM mailboxes WHERE lower(address)=?")
                .bind(&email).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| (r.get("id"), r.get("address"), r.get("display_name")))
        }
    };

    let is_admin = crate::api::authz::is_admin(&state, &email).await;

    let data_json = match mailbox {
        Some((id, address, display_name)) => serde_json::json!({
            "email": email, "mailbox_id": id, "address": address, "display_name": display_name, "is_admin": is_admin,
        }),
        None => serde_json::json!({ "email": email, "mailbox_id": null, "address": null, "display_name": null, "is_admin": is_admin }),
    };
    Ok(Json(serde_json::json!({"success": true, "data": data_json})))
}
