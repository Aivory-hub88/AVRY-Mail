use axum::{extract::Request, http::{HeaderMap, StatusCode}, middleware::Next, response::Response};
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
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

pub fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    let auth = headers.get("authorization")?.to_str().ok()?;
    if auth.to_lowercase().starts_with("bearer ") {
        Some(auth[7..].trim().to_string())
    } else { None }
}

pub fn verify_internal_token(headers: &HeaderMap, expected: &str) -> bool {
    if let Some(v) = headers.get("x-internal-token").and_then(|v| v.to_str().ok()) {
        return v == expected;
    }
    if let Some(bearer) = extract_bearer(headers) {
        return bearer == expected;
    }
    false
}

// Middleware for protected routes — checks JWT or internal token
pub async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::api::AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = req.headers().clone();
    // allow internal token bypass
    if verify_internal_token(&headers, &state.config.internal_token) {
        return Ok(next.run(req).await);
    }
    // try JWT
    if let Some(token) = extract_bearer(&headers) {
        if verify_jwt(&token, &state.config.jwt_secret).is_ok() {
            return Ok(next.run(req).await);
        }
    }
    // allow API key via x-api-key header
    if let Some(api_key) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        // verify against DB would go here; for now check non-empty and let handler verify
        if !api_key.is_empty() {
            req.extensions_mut().insert(ApiKey(api_key.to_string()));
            return Ok(next.run(req).await);
        }
    }
    Err(StatusCode::UNAUTHORIZED)
}

#[derive(Clone, Debug)]
pub struct ApiKey(pub String);
