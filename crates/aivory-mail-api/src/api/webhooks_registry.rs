use std::sync::Arc;
use axum::{extract::{State, Path}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, url, events, secret, enabled, created_at FROM webhooks WHERE tenant_id='default' ORDER BY created_at DESC").fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| {
                let id_str = row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("id").unwrap_or_default());
                let enabled_val = row.try_get::<bool,_>("enabled").map(|b| b).unwrap_or_else(|_| row.try_get::<i32,_>("enabled").map(|i| i!=0).unwrap_or(false));
                let created = row.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at").map(|d| d.to_rfc3339()).unwrap_or_else(|_| row.try_get::<String,_>("created_at").unwrap_or_default());
                serde_json::json!({
                "id": id_str,
                "url": row.get::<String,_>("url"),
                "events": serde_json::from_str::<Value>(&row.get::<String,_>("events")).unwrap_or(Value::Array(vec![])),
                "secret": row.get::<String,_>("secret"),
                "enabled": enabled_val,
                "created_at": created
            })}).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, url, events, secret, enabled, created_at FROM webhooks WHERE tenant_id='default' ORDER BY created_at DESC").fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<String,_>("id"),
                "url": row.get::<String,_>("url"),
                "events": serde_json::from_str::<Value>(&row.get::<String,_>("events")).unwrap_or(Value::Array(vec![])),
                "secret": row.get::<String,_>("secret"),
                "enabled": row.get::<i32,_>("enabled")!=0,
                "created_at": row.get::<String,_>("created_at")
            })).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn create(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let url = body.get("url").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    if !url.starts_with("http") { return Err(StatusCode::BAD_REQUEST); }
    let events = body.get("events").cloned().unwrap_or(serde_json::json!(["email.received"]));
    let events_str = serde_json::to_string(&events).unwrap();
    let secret = body.get("secret").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let enabled = body.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
    let id = Uuid::new_v4();
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO webhooks (id, tenant_id, url, events, secret, enabled, created_at) VALUES ($1,'default',$2,$3,$4,$5,NOW())")
                .bind(id).bind(url).bind(&events_str).bind(&secret).bind(enabled).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO webhooks (id, tenant_id, url, events, secret, enabled, created_at) VALUES (?,?,?,?,?,?,?)")
                .bind(id.to_string()).bind("default").bind(url).bind(&events_str).bind(&secret).bind(if enabled{1}else{0}).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({"success": true, "data": {"id": id.to_string()}}))))
}

pub async fn remove(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("DELETE FROM webhooks WHERE id=$1").bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("DELETE FROM webhooks WHERE id=?").bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn deliveries(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, event, status, attempts, last_error, created_at, next_retry_at FROM webhook_deliveries WHERE webhook_id=$1 ORDER BY created_at DESC LIMIT 50").bind(uid).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| {
                let id_str = row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("id").unwrap_or_default());
                let created = row.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at").map(|d| d.to_rfc3339()).unwrap_or_else(|_| row.try_get::<String,_>("created_at").unwrap_or_default());
                let next_retry = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("next_retry_at").map(|o| o.map(|d| d.to_rfc3339())).unwrap_or_else(|_| row.try_get::<Option<String>,_>("next_retry_at").unwrap_or(None));
                serde_json::json!({
                "id": id_str,
                "event": row.get::<String,_>("event"),
                "status": row.get::<String,_>("status"),
                "attempts": row.get::<i32,_>("attempts"),
                "last_error": row.get::<Option<String>,_>("last_error"),
                "created_at": created,
                "next_retry_at": next_retry
            })}).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, event, status, attempts, last_error, created_at, next_retry_at FROM webhook_deliveries WHERE webhook_id=? ORDER BY created_at DESC LIMIT 50").bind(uid.to_string()).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<String,_>("id"),
                "event": row.get::<String,_>("event"),
                "status": row.get::<String,_>("status"),
                "attempts": row.get::<i32,_>("attempts"),
                "last_error": row.get::<Option<String>,_>("last_error"),
                "created_at": row.get::<String,_>("created_at"),
                "next_retry_at": row.get::<Option<String>,_>("next_retry_at")
            })).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn retry(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let _uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let delivery_id = body.get("delivery_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let did = Uuid::parse_str(delivery_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    // For now just reset status to pending and increment attempts
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE webhook_deliveries SET status='pending', next_retry_at=NOW(), attempts=attempts+1 WHERE id=$1")
                .bind(did).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            // fetch the delivery to retry now fire-and-forget
            let row = sqlx::query("SELECT webhook_id, event, payload FROM webhook_deliveries WHERE id=$1").bind(did).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if let Some(r) = row {
                let wid: Uuid = r.try_get::<Uuid,_>("webhook_id").unwrap_or_else(|_| Uuid::parse_str(&r.try_get::<String,_>("webhook_id").unwrap_or_default()).unwrap_or(Uuid::nil()));
                let event: String = r.get("event");
                let payload: Value = r.get("payload");
                let state_clone = state.clone();
                tokio::spawn(async move { let _ = dispatch_webhook(&state_clone, wid, &event, payload).await; });
            }
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE webhook_deliveries SET status='pending', next_retry_at=?, attempts=attempts+1 WHERE id=?")
                .bind(chrono::Utc::now().to_rfc3339()).bind(did.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let row = sqlx::query("SELECT webhook_id, event, payload FROM webhook_deliveries WHERE id=?").bind(did.to_string()).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if let Some(r) = row {
                let wid = Uuid::parse_str(&r.get::<String,_>("webhook_id")).unwrap_or(Uuid::nil());
                let event: String = r.get("event");
                let payload_str: String = r.get("payload");
                let payload: Value = serde_json::from_str(&payload_str).unwrap_or(Value::Null);
                let state_clone = state.clone();
                tokio::spawn(async move { let _ = dispatch_webhook(&state_clone, wid, &event, payload).await; });
            }
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

// helper to actually dispatch (used by inbound and retry)
pub async fn dispatch_webhook(state: &Arc<AppState>, webhook_id: Uuid, event: &str, payload: Value) -> anyhow::Result<()> {
    let (url, secret): (String, String) = match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT url, secret FROM webhooks WHERE id=$1 AND enabled=true").bind(webhook_id).fetch_optional(pool).await?;
            if let Some(r) = row { (r.get("url"), r.get("secret")) } else { anyhow::bail!("webhook not found/disabled"); }
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT url, secret FROM webhooks WHERE id=? AND enabled=1").bind(webhook_id.to_string()).fetch_optional(pool).await?;
            if let Some(r) = row { (r.get("url"), r.get("secret")) } else { anyhow::bail!("webhook not found/disabled"); }
        }
    };
    let client = reqwest::Client::new();
    let mut req = client.post(&url).json(&serde_json::json!({"event": event, "payload": payload})).timeout(std::time::Duration::from_secs(8));
    if !secret.is_empty() { req = req.header("X-Webhook-Secret", &secret); }
    let resp = req.send().await;
    let (status, err) = match resp {
        Ok(r) if r.status().is_success() => ("delivered", None),
        Ok(r) => ("failed", Some(format!("status {}", r.status()))),
        Err(e) => ("failed", Some(e.to_string())),
    };
    // update delivery row if exists, otherwise create one
    let now = chrono::Utc::now();
    match &state.db {
        DbPool::Postgres(pool) => {
            // we already have delivery row for retry path; for new dispatches we insert
            // try update by webhook_id+payload hash? For now just log
            let _ = sqlx::query("INSERT INTO webhook_deliveries (id, webhook_id, event, payload, status, attempts, last_error, created_at, next_retry_at) VALUES ($1,$2,$3,$4,$5,1,$6,NOW(),NULL) ON CONFLICT DO NOTHING")
                .bind(Uuid::new_v4()).bind(webhook_id).bind(event).bind(&payload).bind(status).bind(&err).execute(pool).await;
            if status == "failed" {
                let _ = sqlx::query("UPDATE webhook_deliveries SET next_retry_at=NOW() + INTERVAL '5 minutes' WHERE webhook_id=$1 AND status='failed' ORDER BY created_at DESC LIMIT 1").bind(webhook_id).execute(pool).await;
            }
            let _ = now;
        }
        DbPool::Sqlite(pool) => {
            let _ = sqlx::query("INSERT INTO webhook_deliveries (id, webhook_id, event, payload, status, attempts, last_error, created_at) VALUES (?,?,?,?,?,?,?,?)")
                .bind(Uuid::new_v4().to_string()).bind(webhook_id.to_string()).bind(event).bind(serde_json::to_string(&payload).unwrap()).bind(status).bind(1).bind(&err).bind(now.to_rfc3339()).execute(pool).await;
        }
    }
    if status == "failed" { anyhow::bail!("webhook failed: {:?}", err); }
    Ok(())
}

pub async fn trigger_for_event(state: &Arc<AppState>, event: &str, payload: Value) {
    // fire all enabled webhooks that subscribe to this event (or wildcard)
    let webhooks: Vec<(Uuid, String)> = match &state.db {
        DbPool::Postgres(pool) => {
            let rows = sqlx::query("SELECT id, events FROM webhooks WHERE enabled=true").fetch_all(pool).await.unwrap_or_default();
            rows.into_iter().filter_map(|r| {
                let id: Uuid = r.get("id");
                let evs: String = r.get("events");
                let v: Value = serde_json::from_str(&evs).unwrap_or(Value::Array(vec![]));
                let arr = v.as_array()?;
                if arr.iter().any(|e| e.as_str()==Some(event) || e.as_str()==Some("*")) { Some((id, evs)) } else { None }
            }).collect()
        }
        DbPool::Sqlite(pool) => {
            let rows = sqlx::query("SELECT id, events FROM webhooks WHERE enabled=1").fetch_all(pool).await.unwrap_or_default();
            rows.into_iter().filter_map(|r| {
                let id_str: String = r.get("id");
                let id = Uuid::parse_str(&id_str).ok()?;
                let evs: String = r.get("events");
                let v: Value = serde_json::from_str(&evs).unwrap_or(Value::Array(vec![]));
                let arr = v.as_array()?;
                if arr.iter().any(|e| e.as_str()==Some(event) || e.as_str()==Some("*")) { Some((id, evs)) } else { None }
            }).collect()
        }
    };
    for (wid, _) in webhooks {
        let state_c = state.clone();
        let ev = event.to_string();
        let pl = payload.clone();
        tokio::spawn(async move { let _ = dispatch_webhook(&state_c, wid, &ev, pl).await; });
    }
}
