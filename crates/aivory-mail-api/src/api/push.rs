//! Web Push subscriptions for the signed-in mailbox, and the fan-out that
//! turns a new Inbox message into a notification on every subscribed browser
//! (see crate::webpush for the protocol).
//!
//! Mailbox identity always comes from the bearer JWT, never from the client:
//! a subscription can only ever receive its own mailbox's mail.

use crate::api::AppState;
use crate::webpush::{self, SendOutcome, Subscription};
use aivory_mail_storage::db::DbPool;
use axum::{extract::State, http::{HeaderMap, StatusCode}, Json};
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

/// A mailbox rarely has more than a handful of browsers; the cap keeps a
/// misbehaving client from filling the table.
const MAX_SUBSCRIPTIONS_PER_MAILBOX: i64 = 20;
/// Longest endpoint we store (real ones are ~200–500 chars).
const MAX_ENDPOINT_LEN: usize = 2048;

async fn own_mailbox_id(state: &Arc<AppState>, headers: &HeaderMap) -> Result<String, StatusCode> {
    let email = crate::api::authz::authenticated_email(state, headers)?;
    let row: Option<String> = match &state.db {
        DbPool::Postgres(pool) => sqlx::query("SELECT id FROM mailboxes WHERE lower(address)=lower($1) LIMIT 1")
            .bind(&email).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .map(|r| r.get::<Uuid, _>("id").to_string()),
        DbPool::Sqlite(pool) => sqlx::query("SELECT id FROM mailboxes WHERE lower(address)=lower(?) LIMIT 1")
            .bind(&email).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .map(|r| r.get::<String, _>("id")),
    };
    row.ok_or(StatusCode::NOT_FOUND)
}

/// GET /v1/push/config — whether push is on here, and the key the browser
/// needs to subscribe (`applicationServerKey`). The key is public by design.
pub async fn config(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    crate::api::authz::authenticated_email(&state, &headers)?;
    Ok(Json(match &state.config.web_push {
        Some(v) => json!({"success": true, "data": {"enabled": true, "public_key": v.public_key_b64}}),
        None => json!({"success": true, "data": {"enabled": false, "public_key": null}}),
    }))
}

#[derive(serde::Deserialize)]
pub struct SubscribeBody {
    endpoint: String,
    keys: SubscribeKeys,
}

#[derive(serde::Deserialize)]
pub struct SubscribeKeys {
    p256dh: String,
    auth: String,
}

/// Checks a client-supplied subscription before it is stored: a real push
/// service host, and keys our encryption can actually use.
pub fn validate_subscription(endpoint: &str, p256dh: &str, auth: &str) -> Result<Subscription, &'static str> {
    if endpoint.len() > MAX_ENDPOINT_LEN || !webpush::is_allowed_endpoint(endpoint) {
        return Err("unsupported push endpoint");
    }
    let sub = Subscription { endpoint: endpoint.to_string(), p256dh: p256dh.to_string(), auth: auth.to_string() };
    // A trial encryption validates both keys exactly the way sending will.
    webpush::encrypt(b"{}", &sub).map_err(|_| "invalid subscription keys")?;
    Ok(sub)
}

/// POST /v1/push/subscriptions — store (or refresh) this browser's
/// subscription for the caller's own mailbox.
pub async fn subscribe(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<SubscribeBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let err = |code: StatusCode, msg: &str| (code, Json(json!({"success": false, "error": msg})));
    if state.config.web_push.is_none() {
        return Err(err(StatusCode::SERVICE_UNAVAILABLE, "push notifications are not enabled on this server"));
    }
    let mailbox_id = own_mailbox_id(&state, &headers).await.map_err(|c| err(c, "unauthorized"))?;
    let sub = validate_subscription(&body.endpoint, &body.keys.p256dh, &body.keys.auth)
        .map_err(|m| err(StatusCode::BAD_REQUEST, m))?;
    let ua: Option<String> = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(200).collect());
    let now = chrono::Utc::now().to_rfc3339();
    let db_err = |_| err(StatusCode::INTERNAL_SERVER_ERROR, "could not save subscription");

    let count: i64 = match &state.db {
        DbPool::Postgres(pool) => sqlx::query_scalar("SELECT COUNT(*) FROM push_subscriptions WHERE mailbox_id=$1 AND endpoint<>$2")
            .bind(&mailbox_id).bind(&sub.endpoint).fetch_one(pool).await.map_err(db_err)?,
        DbPool::Sqlite(pool) => sqlx::query_scalar("SELECT COUNT(*) FROM push_subscriptions WHERE mailbox_id=? AND endpoint<>?")
            .bind(&mailbox_id).bind(&sub.endpoint).fetch_one(pool).await.map_err(db_err)?,
    };
    if count >= MAX_SUBSCRIPTIONS_PER_MAILBOX {
        return Err(err(StatusCode::TOO_MANY_REQUESTS, "too many browsers subscribed for this mailbox"));
    }

    // Same endpoint again (key rotation, or another account in the same
    // browser profile): it now belongs to this mailbox, with fresh keys.
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO push_subscriptions (id, mailbox_id, endpoint, p256dh, auth, user_agent, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (endpoint) DO UPDATE SET mailbox_id=EXCLUDED.mailbox_id, p256dh=EXCLUDED.p256dh, auth=EXCLUDED.auth, user_agent=EXCLUDED.user_agent, failure_count=0")
                .bind(Uuid::new_v4()).bind(&mailbox_id).bind(&sub.endpoint).bind(&sub.p256dh).bind(&sub.auth).bind(&ua).bind(&now)
                .execute(pool).await.map_err(db_err)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO push_subscriptions (id, mailbox_id, endpoint, p256dh, auth, user_agent, created_at) VALUES (?,?,?,?,?,?,?) ON CONFLICT (endpoint) DO UPDATE SET mailbox_id=excluded.mailbox_id, p256dh=excluded.p256dh, auth=excluded.auth, user_agent=excluded.user_agent, failure_count=0")
                .bind(Uuid::new_v4().to_string()).bind(&mailbox_id).bind(&sub.endpoint).bind(&sub.p256dh).bind(&sub.auth).bind(&ua).bind(&now)
                .execute(pool).await.map_err(db_err)?;
        }
    }
    Ok(Json(json!({"success": true})))
}

#[derive(serde::Deserialize)]
pub struct UnsubscribeBody {
    endpoint: String,
}

/// DELETE /v1/push/subscriptions — forget this browser. Only the caller's
/// own mailbox rows can be removed.
pub async fn unsubscribe(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<UnsubscribeBody>,
) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = own_mailbox_id(&state, &headers).await?;
    match &state.db {
        DbPool::Postgres(pool) => sqlx::query("DELETE FROM push_subscriptions WHERE endpoint=$1 AND mailbox_id=$2")
            .bind(&body.endpoint).bind(&mailbox_id).execute(pool).await.map(|_| ()),
        DbPool::Sqlite(pool) => sqlx::query("DELETE FROM push_subscriptions WHERE endpoint=? AND mailbox_id=?")
            .bind(&body.endpoint).bind(&mailbox_id).execute(pool).await.map(|_| ()),
    }
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({"success": true})))
}

/// The mailbox's "New mail banner" toggle (Settings > Notifications).
/// Missing means on, matching the web client's default.
async fn banner_enabled(state: &Arc<AppState>, mailbox_id: &str) -> bool {
    let value: Option<String> = match &state.db {
        DbPool::Postgres(pool) => sqlx::query_scalar("SELECT value FROM user_settings WHERE category='notifications' AND key='new_mail_banner' AND (mailbox_id=$1 OR mailbox_id IS NULL) ORDER BY mailbox_id NULLS LAST LIMIT 1")
            .bind(mailbox_id).fetch_optional(pool).await.ok().flatten(),
        DbPool::Sqlite(pool) => sqlx::query_scalar("SELECT value FROM user_settings WHERE category='notifications' AND key='new_mail_banner' AND (mailbox_id=? OR mailbox_id IS NULL OR mailbox_id='') ORDER BY (mailbox_id IS NULL OR mailbox_id='') LIMIT 1")
            .bind(mailbox_id).fetch_optional(pool).await.ok().flatten(),
    };
    value.map(|v| v != "false").unwrap_or(true)
}

/// The notification a service worker shows. Kept small: push services cap
/// the encrypted payload at ~4 KB.
pub fn new_mail_payload(message_id: &str, from: &str, subject: &str, snippet: &str) -> Value {
    let clip = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    let body = if subject.trim().is_empty() { snippet } else { subject };
    json!({
        "type": "new_mail",
        "message_id": message_id,
        "title": if from.trim().is_empty() { "New message".to_string() } else { clip(from, 120) },
        "body": clip(if body.trim().is_empty() { "You have new mail" } else { body }, 240),
        "url": format!("/?open={message_id}"),
    })
}

/// Push a new Inbox message to every browser subscribed to `mailbox_id`.
/// Fire-and-forget from the inbound path: failures are logged, never block
/// delivery. Dead subscriptions (404/410) are deleted.
pub async fn notify_new_mail(state: Arc<AppState>, mailbox_id: String, payload: Value) {
    let Some(vapid) = state.config.web_push.clone() else { return };
    if !banner_enabled(&state, &mailbox_id).await {
        return;
    }
    let subs: Vec<Subscription> = match &state.db {
        DbPool::Postgres(pool) => sqlx::query("SELECT endpoint, p256dh, auth FROM push_subscriptions WHERE mailbox_id=$1")
            .bind(&mailbox_id).fetch_all(pool).await
            .map(|rows| rows.into_iter().map(|r| Subscription { endpoint: r.get("endpoint"), p256dh: r.get("p256dh"), auth: r.get("auth") }).collect())
            .unwrap_or_default(),
        DbPool::Sqlite(pool) => sqlx::query("SELECT endpoint, p256dh, auth FROM push_subscriptions WHERE mailbox_id=?")
            .bind(&mailbox_id).fetch_all(pool).await
            .map(|rows| rows.into_iter().map(|r| Subscription { endpoint: r.get("endpoint"), p256dh: r.get("p256dh"), auth: r.get("auth") }).collect())
            .unwrap_or_default(),
    };
    if subs.is_empty() {
        return;
    }
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build() {
        Ok(c) => c,
        Err(_) => return,
    };
    for sub in subs {
        let outcome = webpush::send(&client, &vapid, &sub, &payload).await;
        let now = chrono::Utc::now().to_rfc3339();
        let result = match (&outcome, &state.db) {
            (SendOutcome::Delivered, DbPool::Postgres(p)) => sqlx::query("UPDATE push_subscriptions SET last_success_at=$1, failure_count=0 WHERE endpoint=$2").bind(&now).bind(&sub.endpoint).execute(p).await.map(|_| ()),
            (SendOutcome::Delivered, DbPool::Sqlite(p)) => sqlx::query("UPDATE push_subscriptions SET last_success_at=?, failure_count=0 WHERE endpoint=?").bind(&now).bind(&sub.endpoint).execute(p).await.map(|_| ()),
            (SendOutcome::Gone, DbPool::Postgres(p)) => sqlx::query("DELETE FROM push_subscriptions WHERE endpoint=$1").bind(&sub.endpoint).execute(p).await.map(|_| ()),
            (SendOutcome::Gone, DbPool::Sqlite(p)) => sqlx::query("DELETE FROM push_subscriptions WHERE endpoint=?").bind(&sub.endpoint).execute(p).await.map(|_| ()),
            (SendOutcome::Failed(_), DbPool::Postgres(p)) => sqlx::query("UPDATE push_subscriptions SET failure_count=failure_count+1 WHERE endpoint=$1").bind(&sub.endpoint).execute(p).await.map(|_| ()),
            (SendOutcome::Failed(_), DbPool::Sqlite(p)) => sqlx::query("UPDATE push_subscriptions SET failure_count=failure_count+1 WHERE endpoint=?").bind(&sub.endpoint).execute(p).await.map(|_| ()),
        };
        match &outcome {
            SendOutcome::Failed(e) => tracing::warn!("web push to mailbox {mailbox_id} failed: {e}"),
            SendOutcome::Gone => tracing::info!("web push subscription for mailbox {mailbox_id} expired; removed"),
            SendOutcome::Delivered => {}
        }
        if let Err(e) = result {
            tracing::warn!("web push bookkeeping failed: {e}");
        }
    }
}
