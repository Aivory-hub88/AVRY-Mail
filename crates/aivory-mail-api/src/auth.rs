use axum::{
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// `type` claim of a browser session token (issued by `/v1/auth/login`).
pub const TOKEN_TYPE_ACCESS: &str = "access";
/// `type` claim of an OAuth `state` token (see `sign_state_jwt`).
pub const TOKEN_TYPE_OAUTH_STATE: &str = "oauth_state";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user id
    pub tenant_id: Option<String>,
    pub role: Option<String>,
    pub exp: usize,
    /// Which kind of token this is. Sessions and OAuth state share one
    /// HS256 secret, so the signature alone can't tell them apart. Tokens
    /// minted before this claim existed have none (see `is_session_claims`).
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

fn decode_claims(token: &str, secret: &str) -> Result<Claims, String> {
    let key = DecodingKey::from_secret(secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    decode::<Claims>(token, &key, &validation)
        .map(|d| d.claims)
        .map_err(|e| e.to_string())
}

/// Legacy state tokens carried only a purpose `role` such as
/// "calendar_oauth_state".
fn is_state_role(role: Option<&str>) -> bool {
    role.map(|r| r.ends_with("oauth_state") || r.ends_with("oauth-state"))
        .unwrap_or(false)
}

/// A session is a token typed "access", or an untyped legacy login token
/// (7-day sessions issued before `type` existed) that isn't a legacy state
/// token. Anything else typed — OAuth state today — is never a session.
pub fn is_session_claims(claims: &Claims) -> bool {
    match claims.kind.as_deref() {
        Some(kind) => kind == TOKEN_TYPE_ACCESS,
        None => !is_state_role(claims.role.as_deref()),
    }
}

/// Verify a bearer/session token. This is the only verifier for anything
/// that authenticates a user; an OAuth state token is rejected here even
/// though its signature is valid (the state travels through the provider's
/// redirect URL, so it lands in browser history and provider logs).
pub fn verify_session_jwt(token: &str, secret: &str) -> Result<Claims, String> {
    let claims = decode_claims(token, secret)?;
    if !is_session_claims(&claims) {
        return Err("not a session token".into());
    }
    Ok(claims)
}

/// Verify an OAuth `state` token for one purpose (`role`). Sessions and
/// state tokens for other purposes are rejected.
pub fn verify_state_jwt(token: &str, secret: &str, role: &str) -> Result<Claims, String> {
    let claims = decode_claims(token, secret)?;
    if claims.kind.as_deref() != Some(TOKEN_TYPE_OAUTH_STATE) {
        return Err("not an oauth state token".into());
    }
    if claims.role.as_deref() != Some(role) {
        return Err("state token has wrong role".into());
    }
    Ok(claims)
}

/// Short-lived signed token used as an OAuth `state` param (e.g. Google
/// Calendar connect) — carries `sub` through the provider's redirect
/// round-trip with integrity, so the callback can't be tricked into linking
/// a provider account to a different mailbox than the one that started the
/// flow. Reuses the same HS256 secret as session JWTs; `type` marks it as
/// state so `verify_session_jwt` never accepts it, and `role` tags the purpose.
pub fn sign_state_jwt(secret: &str, sub: &str, role: &str, ttl_minutes: i64) -> Result<String, String> {
    let exp = (chrono::Utc::now() + chrono::Duration::minutes(ttl_minutes)).timestamp() as usize;
    let claims = Claims {
        sub: sub.to_string(),
        tenant_id: None,
        role: Some(role.to_string()),
        exp,
        kind: Some(TOKEN_TYPE_OAUTH_STATE.to_string()),
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
        .and_then(|token| verify_session_jwt(&token, &state.config.jwt_secret).ok())
        .is_some()
    {
        return Ok(next.run(req).await);
    }
    Err(StatusCode::UNAUTHORIZED)
}
