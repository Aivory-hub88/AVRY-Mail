//! The inbound webhooks are on the public router (the SMTP ingress and the
//! Cloudflare worker have no user session), so the internal token is checked
//! by the handlers themselves. These tests go through the real router.
//!
//! A request that passes the gate carries an undecodable `raw`, so it ends in
//! 400 before anything is delivered or stored: 401 means "stopped at the
//! gate", 400 means "got past it".

use aivory_mail_api::{
    api::{self, AppState},
    config::Config,
    realtime::RealtimeHub,
};
use aivory_mail_storage::{
    db::DbPool,
    object_store::{LocalStore, ObjectStore},
};
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tower::ServiceExt;

// Config::for_tests uses this internal token.
const TOKEN: &str = "test-internal-token";

async fn app() -> Router {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("sqlite pool");
    let store: Arc<dyn ObjectStore> = Arc::new(LocalStore::new(
        std::env::temp_dir().join("aivory-mail-webhook-auth-tests"),
    ));
    api::router(Arc::new(AppState {
        config: Config::for_tests("sqlite::memory:"),
        db: DbPool::Sqlite(pool),
        store,
        hub: RealtimeHub::new(),
    }))
}

async fn post(app: &Router, path: &str, token: Option<&str>) -> StatusCode {
    let mut req = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        req = req.header("x-internal-token", token);
    }
    let body = serde_json::json!({"from": "a@example.com", "to": "x@example.com", "raw": "%%%not-base64%%%"});
    app.clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn inbound_webhooks_require_the_internal_token_unless_explicitly_relaxed() {
    std::env::remove_var("INBOUND_WEBHOOK_AUTH");
    let app = app().await;

    for path in ["/v1/webhooks/inbound", "/v1/webhooks/cloudflare"] {
        // no token, wrong token, the config-file placeholder, a near miss
        assert_eq!(post(&app, path, None).await, StatusCode::UNAUTHORIZED, "{path}: no token");
        assert_eq!(post(&app, path, Some("wrong")).await, StatusCode::UNAUTHORIZED, "{path}: wrong token");
        assert_eq!(post(&app, path, Some("${INTERNAL_TOKEN}")).await, StatusCode::UNAUTHORIZED, "{path}: placeholder");
        assert_eq!(post(&app, path, Some("test-internal-token ")).await, StatusCode::UNAUTHORIZED, "{path}: near miss");
        // the right token gets past the gate (and then fails on the bad body)
        assert_eq!(post(&app, path, Some(TOKEN)).await, StatusCode::BAD_REQUEST, "{path}: valid token");
    }

    // A browser session is not an internal credential.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v1/webhooks/inbound")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::UNAUTHORIZED);

    // Rollout mode: accepted but logged, and only when asked for explicitly.
    std::env::set_var("INBOUND_WEBHOOK_AUTH", "warn");
    assert_eq!(post(&app, "/v1/webhooks/cloudflare", None).await, StatusCode::BAD_REQUEST);
    assert_eq!(post(&app, "/v1/webhooks/inbound", Some("wrong")).await, StatusCode::BAD_REQUEST);
    std::env::set_var("INBOUND_WEBHOOK_AUTH", "off"); // not a mode: still enforces
    assert_eq!(post(&app, "/v1/webhooks/inbound", None).await, StatusCode::UNAUTHORIZED);
    std::env::remove_var("INBOUND_WEBHOOK_AUTH");
    assert_eq!(post(&app, "/v1/webhooks/inbound", None).await, StatusCode::UNAUTHORIZED);
}
