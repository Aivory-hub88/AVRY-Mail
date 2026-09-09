use crate::api::{audit, authz, AppState};
use aivory_mail_storage::db::DbPool;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::Value;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, StatusCode> {
    let requested = params.get("mailbox_id").and_then(|v| v.as_str());
    // Contacts are mailbox-scoped user data. Unlike an admin overview, this
    // endpoint must never return the tenant-wide contact table when the
    // mailbox selector has not been resolved yet.
    let mailbox_id = authz::mailbox_scope(&state, &headers, requested)
        .await?
        .ok_or(StatusCode::BAD_REQUEST)?;
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, email, display_name, blocked, last_seen_at, mailbox_id FROM contacts WHERE tenant_id::text='default' AND mailbox_id=$1 ORDER BY last_seen_at DESC LIMIT 100")
                .bind(mailbox_id.to_string())
                .fetch_all(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({
                "id": row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.get::<String,_>("id")),
                "email": row.get::<String,_>("email"),
                "display_name": row.get::<String,_>("display_name"),
                "blocked": row.try_get::<bool,_>("blocked").unwrap_or_else(|_| row.try_get::<i32,_>("blocked").map(|i| i!=0).unwrap_or(false)),
                "last_seen_at": row.try_get::<chrono::DateTime<chrono::Utc>,_>("last_seen_at").map(|d| d.to_rfc3339()).unwrap_or_else(|_| row.get::<String,_>("last_seen_at")),
                "mailbox_id": row.get::<Option<String>,_>("mailbox_id")
            })).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, email, display_name, blocked, last_seen_at, mailbox_id FROM contacts WHERE tenant_id='default' AND mailbox_id=? ORDER BY last_seen_at DESC LIMIT 100")
                .bind(mailbox_id.to_string())
                .fetch_all(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter()
                .map(|row| {
                    serde_json::json!({
                        "id": row.get::<String,_>("id"),
                        "email": row.get::<String,_>("email"),
                        "display_name": row.get::<String,_>("display_name"),
                        "blocked": row.get::<i32,_>("blocked") != 0,
                        "last_seen_at": row.get::<String,_>("last_seen_at"),
                        "mailbox_id": row.try_get::<Option<String>,_>("mailbox_id").unwrap_or(None)
                    })
                })
                .collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn block(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = authz::mailbox_scope(
        &state,
        &headers,
        body.get("mailbox_id").and_then(|v| v.as_str()),
    )
    .await?
    .ok_or(StatusCode::BAD_REQUEST)?;
    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_lowercase();
    let display_name = body
        .get("display_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let id = Uuid::new_v4();
    // upsert contact as blocked
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO contacts (id, tenant_id, mailbox_id, email, display_name, blocked, last_seen_at, created_at) VALUES ($1,'default',$2,$3,$4,1,NOW(),NOW()) ON CONFLICT (tenant_id, mailbox_id, email) DO UPDATE SET blocked=1, mailbox_id=COALESCE(contacts.mailbox_id, EXCLUDED.mailbox_id), display_name=EXCLUDED.display_name, last_seen_at=NOW()")
                .bind(id.to_string()).bind(mailbox_id.to_string()).bind(&email).bind(&display_name).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            // also create a mailbox-scoped filter rule
            let fid = Uuid::new_v4();
            let criteria = serde_json::json!({"from": email}).to_string();
            let action = serde_json::json!({"move": "Trash"}).to_string();
            sqlx::query("INSERT INTO mail_filters (id, tenant_id, mailbox_id, name, criteria_json, action_json, enabled, created_at) VALUES ($1,'default',$2,$3,$4,$5,1,NOW()) ON CONFLICT DO NOTHING")
                .bind(fid.to_string()).bind(mailbox_id.to_string()).bind(format!("Block {}", email)).bind(&criteria).bind(&action).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO contacts (id, tenant_id, mailbox_id, email, display_name, blocked, last_seen_at, created_at) VALUES (?,?,?,?,?,?,?,?) ON CONFLICT(tenant_id, mailbox_id, email) DO UPDATE SET blocked=1, mailbox_id=COALESCE(contacts.mailbox_id, excluded.mailbox_id), display_name=excluded.display_name, last_seen_at=excluded.last_seen_at")
                .bind(id.to_string()).bind("default").bind(mailbox_id.to_string()).bind(&email).bind(&display_name).bind(1).bind(&now).bind(&now).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let fid = Uuid::new_v4().to_string();
            let criteria = serde_json::json!({"from": email}).to_string();
            let action = serde_json::json!({"move": "Trash"}).to_string();
            sqlx::query("INSERT OR IGNORE INTO mail_filters (id, tenant_id, mailbox_id, name, criteria_json, action_json, enabled, created_at) VALUES (?,?,?,?,?,?,?,?)")
                .bind(&fid).bind("default").bind(mailbox_id.to_string()).bind(format!("Block {}", email)).bind(&criteria).bind(&action).bind(1).bind(&now).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    let db2 = state.db.clone();
    let em = email.clone();
    tokio::spawn(async move {
        audit::log(&db2, "contact.block", None, Some(&em), None, None, None).await;
    });
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn import_contacts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = authz::mailbox_scope(
        &state,
        &headers,
        body.get("mailbox_id").and_then(|v| v.as_str()),
    )
    .await?
    .ok_or(StatusCode::BAD_REQUEST)?;
    // Accept {contacts:[{email, display_name/name}]} or {csv:"email,name\n..."} or plain array
    let mut list: Vec<(String, String)> = Vec::new();
    if let Some(arr) = body.get("contacts").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(email) = item.get("email").and_then(|v| v.as_str()) {
                let name = item
                    .get("display_name")
                    .or(item.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !email.is_empty() {
                    list.push((email.to_string(), name));
                }
            }
        }
    } else if let Some(csv) = body.get("csv").and_then(|v| v.as_str()) {
        for line in csv.lines() {
            let line = line.trim();
            if line.is_empty() || line.to_lowercase().starts_with("email") {
                continue;
            }
            let parts: Vec<&str> = line.split(',').collect();
            if parts.is_empty() {
                continue;
            }
            let email = parts[0].trim().to_string();
            let name = parts.get(1).unwrap_or(&"").trim().to_string();
            if email.contains('@') {
                list.push((email, name));
            }
        }
    } else if let Some(arr) = body.as_array() {
        for item in arr {
            if let Some(email) = item.get("email").and_then(|v| v.as_str()) {
                let name = item
                    .get("display_name")
                    .or(item.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                list.push((email.to_string(), name));
            }
        }
    } else {
        return Err(StatusCode::BAD_REQUEST);
    }
    if list.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut imported = 0;
    for (email, name) in list {
        let email_lc = email.to_lowercase();
        if !email_lc.contains('@') {
            continue;
        }
        // reuse upsert
        upsert_from_address(&state.db, &mailbox_id, &email_lc, &name).await;
        imported += 1;
    }
    Ok(Json(
        serde_json::json!({"success": true, "data": {"imported": imported}}),
    ))
}

pub async fn upsert_from_address(db: &DbPool, mailbox_id: &Uuid, email: &str, display_name: &str) {
    let email = email.to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return;
    }
    let now = chrono::Utc::now().to_rfc3339();
    let id = Uuid::new_v4();
    match db {
        DbPool::Postgres(pool) => {
            let _ = sqlx::query("INSERT INTO contacts (id, tenant_id, mailbox_id, email, display_name, blocked, last_seen_at, created_at) VALUES ($1,'default',$2,$3,$4,0,NOW(),NOW()) ON CONFLICT (tenant_id, mailbox_id, email) DO UPDATE SET display_name=CASE WHEN contacts.display_name='' THEN EXCLUDED.display_name ELSE contacts.display_name END, mailbox_id=COALESCE(contacts.mailbox_id, EXCLUDED.mailbox_id), last_seen_at=NOW()")
                .bind(id.to_string()).bind(mailbox_id.to_string()).bind(&email).bind(display_name).execute(pool).await;
        }
        DbPool::Sqlite(pool) => {
            let _ = sqlx::query("INSERT INTO contacts (id, tenant_id, mailbox_id, email, display_name, blocked, last_seen_at, created_at) VALUES (?,?,?,?,?,?,?,?) ON CONFLICT(tenant_id, mailbox_id, email) DO UPDATE SET display_name=CASE WHEN display_name='' THEN excluded.display_name ELSE display_name END, mailbox_id=COALESCE(contacts.mailbox_id, excluded.mailbox_id), last_seen_at=excluded.last_seen_at")
                .bind(id.to_string()).bind("default").bind(mailbox_id.to_string()).bind(&email).bind(display_name).bind(0).bind(&now).bind(&now).execute(pool).await;
        }
    }
}
