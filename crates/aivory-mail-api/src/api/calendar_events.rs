use std::sync::Arc;
use axum::{extract::{State, Path, Query}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

pub async fn list(State(state): State<Arc<AppState>>, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = q.get("mailbox_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let from = q.get("from").and_then(|v| v.as_str());
    let to = q.get("to").and_then(|v| v.as_str());
    let calendar = q.get("calendar").and_then(|v| v.as_str());
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, calendar, title, description, start_at, end_at, guests, color, location, conferencing, conferencing_link FROM calendar_events WHERE mailbox_id=$1 ORDER BY start_at ASC LIMIT 100")
                .bind(mailbox_id).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| {
                serde_json::json!({"id": row.get::<Uuid,_>("id").to_string(), "calendar": row.get::<String,_>("calendar"), "title": row.get::<String,_>("title"), "description": row.get::<String,_>("description"), "start_at": row.get::<String,_>("start_at"), "end_at": row.get::<String,_>("end_at"), "guests": row.get::<String,_>("guests"), "color": row.get::<String,_>("color"), "location": row.get::<String,_>("location"), "conferencing": row.get::<String,_>("conferencing"), "conferencing_link": row.get::<String,_>("conferencing_link")})
            }).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, calendar, title, description, start_at, end_at, guests, color, location, conferencing, conferencing_link FROM calendar_events WHERE mailbox_id=? ORDER BY start_at ASC LIMIT 100")
                .bind(mailbox_id).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| {
                serde_json::json!({"id": row.get::<String,_>("id"), "calendar": row.get::<String,_>("calendar"), "title": row.get::<String,_>("title"), "description": row.get::<String,_>("description"), "start_at": row.get::<String,_>("start_at"), "end_at": row.get::<String,_>("end_at"), "guests": row.get::<String,_>("guests"), "color": row.get::<String,_>("color"), "location": row.get::<String,_>("location"), "conferencing": row.get::<String,_>("conferencing"), "conferencing_link": row.get::<String,_>("conferencing_link")})
            }).collect()
        }
    };
    let filtered: Vec<Value> = rows.into_iter().filter(|v| {
        if let Some(cal) = calendar { if v.get("calendar").and_then(|x| x.as_str()) != Some(cal) { return false; } }
        if let (Some(f), Some(s)) = (from, v.get("start_at").and_then(|x| x.as_str())) { if s < f { return false; } }
        if let (Some(t), Some(s)) = (to, v.get("start_at").and_then(|x| x.as_str())) { if s > t { return false; } }
        true
    }).collect();
    Ok(Json(serde_json::json!({"success": true, "data": filtered})))
}

pub async fn create(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let mailbox_id = body.get("mailbox_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?.to_string();
    let tenant_id = body.get("tenant_id").and_then(|v| v.as_str()).unwrap_or("default").to_string();
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("No title").to_string();
    if title.trim().is_empty() { return Err(StatusCode::BAD_REQUEST); }
    let id = body.get("id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()).unwrap_or(Uuid::new_v4());
    let calendar = body.get("calendar").and_then(|v| v.as_str()).unwrap_or("My calendar").to_string();
    let desc = body.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let start_at = body.get("start_at").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?.to_string();
    let end_at = body.get("end_at").and_then(|v| v.as_str()).unwrap_or(&start_at).to_string();
    let guests = serde_json::to_string(body.get("guests").unwrap_or(&serde_json::Value::Array(vec![]))).unwrap();
    let color = body.get("color").and_then(|v| v.as_str()).unwrap_or("blue").to_string();
    let location = body.get("location").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let conferencing = body.get("conferencing").and_then(|v| v.as_str()).unwrap_or("none").to_string();
    let conferencing_link = body.get("conferencing_link").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let recurring = body.get("recurring").and_then(|v| v.as_str()).unwrap_or("never").to_string();
    let notifications = body.get("notifications").and_then(|v| v.as_str()).unwrap_or("10m").to_string();
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO calendar_events (id, tenant_id, mailbox_id, calendar, title, description, start_at, end_at, guests, color, location, conferencing, conferencing_link, recurring, notifications, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,NOW())")
                .bind(id).bind(&tenant_id).bind(&mailbox_id).bind(&calendar).bind(&title).bind(&desc).bind(&start_at).bind(&end_at).bind(&guests).bind(&color).bind(&location).bind(&conferencing).bind(&conferencing_link).bind(&recurring).bind(&notifications).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO calendar_events (id, tenant_id, mailbox_id, calendar, title, description, start_at, end_at, guests, color, location, conferencing, conferencing_link, recurring, notifications, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id.to_string()).bind(&tenant_id).bind(&mailbox_id).bind(&calendar).bind(&title).bind(&desc).bind(&start_at).bind(&end_at).bind(&guests).bind(&color).bind(&location).bind(&conferencing).bind(&conferencing_link).bind(&recurring).bind(&notifications).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({"success": true, "data": {"id": id.to_string()}}))))
}

pub async fn update(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let mailbox_id = body.get("mailbox_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?.to_string();
    let title = body.get("title").and_then(|v| v.as_str());
    let start_at = body.get("start_at").and_then(|v| v.as_str());
    let end_at = body.get("end_at").and_then(|v| v.as_str());
    let calendar = body.get("calendar").and_then(|v| v.as_str());
    match &state.db {
        DbPool::Postgres(pool) => {
            if let Some(t) = title { sqlx::query("UPDATE calendar_events SET title=$1 WHERE id=$2 AND mailbox_id=$3").bind(t).bind(uid).bind(&mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(s) = start_at { sqlx::query("UPDATE calendar_events SET start_at=$1 WHERE id=$2 AND mailbox_id=$3").bind(s).bind(uid).bind(&mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(e) = end_at { sqlx::query("UPDATE calendar_events SET end_at=$1 WHERE id=$2 AND mailbox_id=$3").bind(e).bind(uid).bind(&mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(c) = calendar { sqlx::query("UPDATE calendar_events SET calendar=$1 WHERE id=$2 AND mailbox_id=$3").bind(c).bind(uid).bind(&mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        }
        DbPool::Sqlite(pool) => {
            if let Some(t) = title { sqlx::query("UPDATE calendar_events SET title=? WHERE id=? AND mailbox_id=?").bind(t).bind(uid.to_string()).bind(&mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(s) = start_at { sqlx::query("UPDATE calendar_events SET start_at=? WHERE id=? AND mailbox_id=?").bind(s).bind(uid.to_string()).bind(&mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(e) = end_at { sqlx::query("UPDATE calendar_events SET end_at=? WHERE id=? AND mailbox_id=?").bind(e).bind(uid.to_string()).bind(&mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(c) = calendar { sqlx::query("UPDATE calendar_events SET calendar=? WHERE id=? AND mailbox_id=?").bind(c).bind(uid.to_string()).bind(&mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn remove(State(state): State<Arc<AppState>>, Path(id): Path<String>, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let mailbox_id = q.get("mailbox_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("DELETE FROM calendar_events WHERE id=$1 AND mailbox_id=$2").bind(uid).bind(mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("DELETE FROM calendar_events WHERE id=? AND mailbox_id=?").bind(uid.to_string()).bind(mailbox_id).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}
