use std::sync::Arc;
use axum::{extract::State, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use jsonwebtoken::{encode, Header, EncodingKey};
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

    // Check if mailbox exists (allow login as any mailbox user with admin_password)
    let mailbox_exists = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM mailboxes WHERE lower(address)=$1")
                .bind(&email)
                .fetch_one(pool)
                .await
                .unwrap_or(0) > 0
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM mailboxes WHERE lower(address)=?")
                .bind(&email)
                .fetch_one(pool)
                .await
                .unwrap_or(0) > 0
        }
    };

    // Allow: admin match OR (mailbox exists + correct admin_password) OR demo fallback (admin@aivory.id)
    let allowed = is_admin_match || (mailbox_exists && password == admin_password) || (email == "admin@aivory.id" && password == "aivory123");

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

pub async fn me(State(_state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    Ok(Json(serde_json::json!({"success": true})))
}
