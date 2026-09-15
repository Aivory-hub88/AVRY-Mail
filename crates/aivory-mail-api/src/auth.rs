use axum::{
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user id
    pub tenant_id: Option<String>,
    pub role: Option<String>,
    pub exp: usize,
}

pub fn verify_jwt(token: &str, secret: &str) -> Result<Claims, String> {
    let key = DecodingKey::from_secret(secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    decode::<Claims>(token, &key, &validation)
        .map(|d| d.claims)
        .map_err(|e| e.to_string())
}

/// Short-lived signed token used as an OAuth `state` param (e.g. Google
/// Calendar connect) — carries `sub` through the provider's redirect
/// round-trip with integrity, so the callback can't be tricked into linking
/// a provider account to a different mailbox than the one that started the
/// flow. Reuses the same HS256 secret as session JWTs; `role` tags the
/// purpose so it can never be accepted as a session token or vice versa.
pub fn sign_state_jwt(secret: &str, sub: &str, role: &str, ttl_minutes: i64) -> Result<String, String> {
    let exp = (chrono::Utc::now() + chrono::Duration::minutes(ttl_minutes)).timestamp() as usize;
    let claims = Claims {
        sub: sub.to_string(),
        tenant_id: None,
        role: Some(role.to_string()),
        exp,
    };
    encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|e| e.to_string())
}

pub fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    let auth = headers.get("authorization")?.to_str().ok()?;
    if auth.to_lowercase().starts_with("bearer ") {
        Some(auth[7..].trim().to_string())
    } else {
        None
    }
}

/// Internal credentials are deliberately separate from user Bearer tokens.
/// Only the dedicated header is accepted so a leaked service token cannot be
/// confused with a browser session JWT.
pub fn verify_internal_token(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get("x-internal-token")
        .and_then(|v| v.to_str().ok())
        .map(|value| !expected.is_empty() && value == expected)
        .unwrap_or(false)
}

// Middleware for protected routes — checks a real JWT only. Internal callers
// use explicit endpoint gates (SMTP/webhooks/MCP), never a global bypass.
pub async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::api::AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = req.headers();
    if extract_bearer(headers)
        .and_then(|token| verify_jwt(&token, &state.config.jwt_secret).ok())
        .is_some()
    {
        return Ok(next.run(req).await);
    }
    Err(StatusCode::UNAUTHORIZED)
}
