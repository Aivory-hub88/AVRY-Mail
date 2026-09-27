//! Web Push subscription endpoints: a browser can only subscribe its own
//! mailbox, only to a real push service, and push stays off without keys.

use aivory_mail_api::{api::{self, AppState}, config::Config, realtime::RealtimeHub, webpush::Vapid};
use aivory_mail_storage::{db::DbPool, object_store::{LocalStore, ObjectStore}};
use axum::{body::{to_bytes, Body}, http::{header, Method, Request, StatusCode}, Router};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64URL, Engine as _};
use jsonwebtoken::{encode, EncodingKey, Header};
use p256::{elliptic_curve::sec1::ToEncodedPoint, SecretKey};
use serde_json::{json, Value};
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use std::sync::Arc;
use tower::ServiceExt;

const VAPID_PRIVATE: &str = "yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw"; // RFC 8291 test key
const MAILBOX_A: &str = "mbx-a";
const MAILBOX_B: &str = "mbx-b";

async fn setup(push_enabled: bool) -> (Router, SqlitePool, String) {
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    for sql in [
        "CREATE TABLE mailboxes (id TEXT PRIMARY KEY, address TEXT UNIQUE NOT NULL)",
        "CREATE TABLE push_subscriptions (id TEXT PRIMARY KEY, mailbox_id TEXT NOT NULL, endpoint TEXT NOT NULL UNIQUE, p256dh TEXT NOT NULL, auth TEXT NOT NULL, user_agent TEXT, created_at TEXT NOT NULL, last_success_at TEXT, failure_count INTEGER NOT NULL DEFAULT 0)",
        "INSERT INTO mailboxes (id, address) VALUES ('mbx-a', 'a@test.local'), ('mbx-b', 'b@test.local')",
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    let mut config = Config::for_tests("sqlite::memory:");
    if push_enabled {
        config.web_push = Some(Vapid::from_private_key(VAPID_PRIVATE, "mailto:ops@test.local").unwrap());
    }
    let secret = config.jwt_secret.clone();
    let store: Arc<dyn ObjectStore> = Arc::new(LocalStore::new(std::env::temp_dir().join("aivory-mail-push-tests")));
    let state = Arc::new(AppState { config, db: DbPool::Sqlite(pool.clone()), store, hub: RealtimeHub::new() });
    (api::router(state), pool, secret)
}

fn session(secret: &str, email: &str) -> String {
    let exp = (chrono::Utc::now() + chrono::Duration::minutes(10)).timestamp();
    encode(&Header::default(), &json!({"sub": email, "email": email, "exp": exp, "iat": 0, "type": "access"}), &EncodingKey::from_secret(secret.as_bytes())).unwrap()
}

fn browser_keys() -> Value {
    let ua = SecretKey::random(&mut rand::rngs::OsRng);
    json!({"p256dh": B64URL.encode(ua.public_key().to_encoded_point(false).as_bytes()), "auth": B64URL.encode([9u8; 16])})
}

async fn call(app: &Router, method: Method, uri: &str, token: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri).header(header::AUTHORIZATION, format!("Bearer {token}"));
    if body.is_some() {
        req = req.header(header::CONTENT_TYPE, "application/json");
    }
    let req = req.body(body.map(|b| Body::from(b.to_string())).unwrap_or_else(Body::empty)).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn rows(pool: &SqlitePool) -> Vec<(String, String)> {
    sqlx::query("SELECT mailbox_id, endpoint FROM push_subscriptions ORDER BY endpoint")
        .fetch_all(pool).await.unwrap()
        .into_iter().map(|r| (r.get("mailbox_id"), r.get("endpoint"))).collect()
}

const FCM: &str = "https://fcm.googleapis.com/fcm/send/device-1";

#[tokio::test]
async fn config_exposes_the_public_key_only_when_enabled() {
    let (app, _, secret) = setup(true).await;
    let (status, body) = call(&app, Method::GET, "/v1/push/config", &session(&secret, "a@test.local"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["enabled"], true);
    assert!(body["data"]["public_key"].as_str().unwrap().starts_with("BP4z9KsN6nGRTbVYI"));

    let (app, _, secret) = setup(false).await;
    let (_, body) = call(&app, Method::GET, "/v1/push/config", &session(&secret, "a@test.local"), None).await;
    assert_eq!(body["data"]["enabled"], false);
    let (status, _) = call(&app, Method::POST, "/v1/push/subscriptions", &session(&secret, "a@test.local"), Some(json!({"endpoint": FCM, "keys": browser_keys()}))).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn subscription_is_bound_to_the_callers_own_mailbox() {
    let (app, pool, secret) = setup(true).await;
    // A mailbox_id in the body is ignored: identity comes from the token.
    let (status, _) = call(&app, Method::POST, "/v1/push/subscriptions", &session(&secret, "a@test.local"),
        Some(json!({"endpoint": FCM, "keys": browser_keys(), "mailbox_id": MAILBOX_B}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rows(&pool).await, vec![(MAILBOX_A.to_string(), FCM.to_string())]);

    // Same browser again (key rotation): updated in place, not duplicated.
    let (status, _) = call(&app, Method::POST, "/v1/push/subscriptions", &session(&secret, "a@test.local"),
        Some(json!({"endpoint": FCM, "keys": browser_keys()}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rows(&pool).await.len(), 1);
}

#[tokio::test]
async fn only_real_push_services_and_valid_keys_are_stored() {
    let (app, pool, secret) = setup(true).await;
    let token = session(&secret, "a@test.local");
    for endpoint in ["https://evil.example/collect", "http://fcm.googleapis.com/x", "https://localhost/x"] {
        let (status, _) = call(&app, Method::POST, "/v1/push/subscriptions", &token, Some(json!({"endpoint": endpoint, "keys": browser_keys()}))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{endpoint}");
    }
    let (status, _) = call(&app, Method::POST, "/v1/push/subscriptions", &token,
        Some(json!({"endpoint": FCM, "keys": {"p256dh": "nope", "auth": "nope"}}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(rows(&pool).await.is_empty());
}

#[tokio::test]
async fn unsubscribe_cannot_remove_another_mailboxs_browser() {
    let (app, pool, secret) = setup(true).await;
    call(&app, Method::POST, "/v1/push/subscriptions", &session(&secret, "a@test.local"), Some(json!({"endpoint": FCM, "keys": browser_keys()}))).await;
    let (status, _) = call(&app, Method::DELETE, "/v1/push/subscriptions", &session(&secret, "b@test.local"), Some(json!({"endpoint": FCM}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rows(&pool).await.len(), 1, "b must not delete a's subscription");
    call(&app, Method::DELETE, "/v1/push/subscriptions", &session(&secret, "a@test.local"), Some(json!({"endpoint": FCM}))).await;
    assert!(rows(&pool).await.is_empty());
}

#[tokio::test]
async fn anonymous_callers_are_rejected() {
    let (app, _, _) = setup(true).await;
    let req = Request::get("/v1/push/config").body(Body::empty()).unwrap();
    assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn notification_payload_is_small_and_falls_back_sensibly() {
    let p = api::push::new_mail_payload("m1", "Alvin", "", "draft ready");
    assert_eq!(p["title"], "Alvin");
    assert_eq!(p["body"], "draft ready");
    assert_eq!(p["url"], "/?open=m1");
    let long = "x".repeat(10_000);
    let p = api::push::new_mail_payload("m2", &long, &long, "");
    assert!(p.to_string().len() < aivory_mail_api::webpush::MAX_PAYLOAD_BYTES);
    assert_eq!(api::push::new_mail_payload("m3", " ", "", "")["title"], "New message");
}
