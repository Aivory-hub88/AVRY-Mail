//! An OAuth `state` token must never work as a session, and a session must
//! never work as OAuth state. Both are HS256 under the same JWT_SECRET, so
//! before the `type` claim a calendar state token (which travels through
//! Google's redirect URL) passed every session check.

use aivory_mail_api::{
    api::{self, AppState},
    auth::{self, Claims, TOKEN_TYPE_ACCESS},
    config::Config,
    realtime::RealtimeHub,
};
use aivory_mail_storage::{
    db::DbPool,
    object_store::{LocalStore, ObjectStore},
};
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tower::ServiceExt;

const STATE_ROLE: &str = "calendar_oauth_state";

async fn app() -> (Router, String) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("sqlite pool");
    sqlx::query("CREATE TABLE mailboxes (id TEXT PRIMARY KEY, address TEXT UNIQUE NOT NULL, display_name TEXT, avatar_content_type TEXT)")
        .execute(&pool)
        .await
        .expect("mailboxes table");
    sqlx::query("CREATE TABLE domains (id TEXT PRIMARY KEY, admin_email TEXT)")
        .execute(&pool)
        .await
        .expect("domains table");
    let store: Arc<dyn ObjectStore> = Arc::new(LocalStore::new(
        std::env::temp_dir().join("aivory-mail-oauth-state-tests"),
    ));
    let config = Config::for_tests("sqlite::memory:");
    let secret = config.jwt_secret.clone();
    let state = Arc::new(AppState {
        config,
        db: DbPool::Sqlite(pool),
        store,
        hub: RealtimeHub::new(),
    });
    (api::router(state), secret)
}

fn mint(secret: &str, claims: serde_json::Value) -> String {
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes())).expect("jwt")
}

fn exp() -> usize {
    (chrono::Utc::now() + chrono::Duration::minutes(10)).timestamp() as usize
}

/// What `/v1/auth/login` issues today.
fn session(secret: &str) -> String {
    mint(secret, serde_json::json!({"sub": "user@test.local", "email": "user@test.local", "exp": exp(), "iat": 0, "type": "access"}))
}

/// A 7-day session issued before `type` existed.
fn legacy_session(secret: &str) -> String {
    mint(secret, serde_json::json!({"sub": "user@test.local", "email": "user@test.local", "exp": exp(), "iat": 0}))
}

/// A calendar state token issued before `type` existed (role only).
fn legacy_state(secret: &str) -> String {
    mint(secret, serde_json::json!({"sub": "user@test.local", "tenant_id": null, "role": STATE_ROLE, "exp": exp()}))
}

async fn status(app: &Router, uri: &str, bearer: Option<&str>) -> StatusCode {
    let mut req = Request::get(uri).body(Body::empty()).expect("request");
    if let Some(token) = bearer {
        req.headers_mut()
            .insert(header::AUTHORIZATION, format!("Bearer {token}").parse().expect("header"));
    }
    app.clone().oneshot(req).await.expect("response").status()
}

#[test]
fn session_verifier_rejects_state_tokens() {
    let secret = "unit-test-secret";
    let state = auth::sign_state_jwt(secret, "user@test.local", STATE_ROLE, 10).expect("state");
    assert!(auth::verify_session_jwt(&state, secret).is_err());
    assert!(auth::verify_session_jwt(&legacy_state(secret), secret).is_err());
    assert!(auth::verify_session_jwt(&session(secret), secret).is_ok());
    assert!(auth::verify_session_jwt(&legacy_session(secret), secret).is_ok());
}

#[test]
fn state_verifier_rejects_sessions_and_other_purposes() {
    let secret = "unit-test-secret";
    let state = auth::sign_state_jwt(secret, "mbx-1", STATE_ROLE, 10).expect("state");
    assert_eq!(auth::verify_state_jwt(&state, secret, STATE_ROLE).expect("valid").sub, "mbx-1");
    assert!(auth::verify_state_jwt(&state, secret, "other_oauth_state").is_err());
    assert!(auth::verify_state_jwt(&session(secret), secret, STATE_ROLE).is_err());
    assert!(auth::verify_state_jwt(&legacy_session(secret), secret, STATE_ROLE).is_err());
}

#[test]
fn unknown_token_types_are_not_sessions() {
    let base = Claims { sub: "u".into(), tenant_id: None, role: None, exp: exp(), kind: None };
    assert!(auth::is_session_claims(&base));
    assert!(auth::is_session_claims(&Claims { kind: Some(TOKEN_TYPE_ACCESS.into()), ..base.clone() }));
    assert!(!auth::is_session_claims(&Claims { kind: Some("share".into()), ..base.clone() }));
    // Roles that aren't OAuth state (e.g. admin) stay valid on legacy tokens.
    assert!(auth::is_session_claims(&Claims { role: Some("admin".into()), ..base }));
}

#[tokio::test]
async fn state_token_is_rejected_on_session_routes() {
    let (app, secret) = app().await;
    let state = auth::sign_state_jwt(&secret, "user@test.local", STATE_ROLE, 10).expect("state");

    assert_eq!(status(&app, "/v1/auth/me", Some(&state)).await, StatusCode::UNAUTHORIZED);
    assert_eq!(status(&app, "/v1/auth/me", Some(&legacy_state(&secret))).await, StatusCode::UNAUTHORIZED);
    assert_eq!(
        status(&app, &format!("/v1/calendar/google/connect?token={state}"), None).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn real_sessions_still_pass_session_routes() {
    let (app, secret) = app().await;
    for token in [session(&secret), legacy_session(&secret)] {
        assert_eq!(status(&app, "/v1/auth/me", Some(&token)).await, StatusCode::OK);
    }
}
