use std::sync::Arc;
use axum::{extract::{State, Path, Query}, Json, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;
use sqlx::Row;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

// Generic settings: GET /v1/settings?category=general&mailbox_id=xxx , POST {category,key,value}
pub async fn get(State(state): State<Arc<AppState>>, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let category = q.get("category").and_then(|v| v.as_str());
    let mailbox_id = q.get("mailbox_id").and_then(|v| v.as_str());
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = if let (Some(cat), Some(mb)) = (category, mailbox_id) {
                let uid = Uuid::parse_str(mb).ok();
                sqlx::query("SELECT category, key, value FROM user_settings WHERE category=$1 AND (mailbox_id=$2 OR mailbox_id IS NULL)").bind(cat).bind(uid).fetch_all(pool).await
            } else if let Some(cat) = category {
                sqlx::query("SELECT category, key, value FROM user_settings WHERE category=$1").bind(cat).fetch_all(pool).await
            } else {
                sqlx::query("SELECT category, key, value FROM user_settings").fetch_all(pool).await
            };
            let r = r.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({"category": row.get::<String,_>("category"), "key": row.get::<String,_>("key"), "value": row.get::<String,_>("value")})).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = if let Some(cat) = category {
                sqlx::query("SELECT category, key, value FROM user_settings WHERE category=?").bind(cat).fetch_all(pool).await
            } else {
                sqlx::query("SELECT category, key, value FROM user_settings").fetch_all(pool).await
            };
            let r = r.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({"category": row.get::<String,_>("category"), "key": row.get::<String,_>("key"), "value": row.get::<String,_>("value")})).collect()
        }
    };
    // Seed defaults if empty for category
    let mut defaults = default_for(category);
    for row in &rows {
        if let (Some(k), Some(v)) = (row.get("key").and_then(|x| x.as_str()), row.get("value").and_then(|x| x.as_str())) {
            defaults.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        }
    }
    Ok(Json(serde_json::json!({"success": true, "data": defaults})))
}

pub async fn set(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let category = body.get("category").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let key = body.get("key").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let value = body.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mailbox_id = body.get("mailbox_id").and_then(|v| v.as_str());
    let id = Uuid::new_v4();
    match &state.db {
        DbPool::Postgres(pool) => {
            let mb = mailbox_id.and_then(|s| Uuid::parse_str(s).ok());
            sqlx::query("INSERT INTO user_settings (id, tenant_id, mailbox_id, category, key, value, updated_at) VALUES ($1,'default',$2,$3,$4,$5,NOW()) ON CONFLICT (tenant_id, mailbox_id, category, key) DO UPDATE SET value=$5, updated_at=NOW()")
                .bind(id).bind(mb).bind(category).bind(key).bind(&value).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT OR REPLACE INTO user_settings (id, tenant_id, mailbox_id, category, key, value, updated_at) VALUES (?,?,?,?,?,?,?)")
                .bind(id.to_string()).bind("default").bind(mailbox_id.unwrap_or("")).bind(category).bind(key).bind(&value).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

fn default_for(category: Option<&str>) -> std::collections::HashMap<String, Value> {
    let mut m = std::collections::HashMap::new();
    match category {
        Some("general") => {
            m.insert("undo_send_seconds".into(), Value::String("10".into()));
            m.insert("density".into(), Value::String("comfortable".into()));
            m.insert("conversation_view".into(), Value::String("true".into()));
            m.insert("page_size".into(), Value::String("20".into()));
            m.insert("language".into(), Value::String("en".into()));
        }
        Some("inbox") => {
            m.insert("inbox_type".into(), Value::String("Default".into()));
            m.insert("categories".into(), Value::String("Primary,Promotions,Social".into()));
        }
        Some("compose") => {
            m.insert("default_font".into(), Value::String("Manrope".into()));
            m.insert("font_size".into(), Value::String("14".into()));
            m.insert("font_color".into(), Value::String("#111827".into()));
            m.insert("compose_format".into(), Value::String("html".into()));
            m.insert("always_show_cc".into(), Value::String("false".into()));
            m.insert("always_show_bcc".into(), Value::String("false".into()));
            m.insert("always_show_from".into(), Value::String("false".into()));
            m.insert("outbox_delay_minutes".into(), Value::String("0".into()));
        }
        Some("appearance") => {
            m.insert("theme".into(), Value::String("light".into()));
            m.insert("density".into(), Value::String("comfortable".into()));
            m.insert("reading_pane".into(), Value::String("right".into()));
        }
        Some("notifications") => {
            m.insert("desktop_sound".into(), Value::String("true".into()));
            m.insert("new_mail_banner".into(), Value::String("true".into()));
            m.insert("email_notifications".into(), Value::String("all".into()));
        }
        Some("shortcuts") => {
            m.insert("enabled".into(), Value::String("true".into()));
            m.insert("custom".into(), Value::String("{}".into()));
        }
        Some("storage") => {
            m.insert("days_to_sync".into(), Value::String("30".into()));
            m.insert("auto_archive_days".into(), Value::String("0".into()));
            m.insert("download_attachments_wifi_only".into(), Value::String("true".into()));
        }
        Some("forwarding") => {
            m.insert("forward_to".into(), Value::String("".into()));
            m.insert("keep_copy".into(), Value::String("true".into()));
            m.insert("pop_enabled".into(), Value::String("false".into()));
            m.insert("imap_enabled".into(), Value::String("true".into()));
        }
        _ => {}
    }
    m
}

// Labels CRUD
pub async fn list_labels(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, name, color FROM mail_labels ORDER BY name").fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<Uuid,_>("id").to_string(), "name": row.get::<String,_>("name"), "color": row.get::<String,_>("color")})).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, name, color FROM mail_labels ORDER BY name").fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<String,_>("id"), "name": row.get::<String,_>("name"), "color": row.get::<String,_>("color")})).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}
pub async fn create_label(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let name = body.get("name").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let color = body.get("color").and_then(|v| v.as_str()).unwrap_or("#3b82f6");
    let id = Uuid::new_v4();
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("INSERT INTO mail_labels (id, tenant_id, name, color, created_at) VALUES ($1,'default',$2,$3,NOW())").bind(id).bind(name).bind(color).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("INSERT INTO mail_labels (id, tenant_id, name, color, created_at) VALUES (?,?,?,?,?)").bind(id.to_string()).bind("default").bind(name).bind(color).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({"success": true, "data": {"id": id.to_string()}}))))
}
pub async fn delete_label(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("DELETE FROM mail_labels WHERE id=$1").bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("DELETE FROM mail_labels WHERE id=?").bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

// Filters — priority + reject/block/forward (Mailflare routing parity)
pub async fn list_filters(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, name, criteria_json, action_json, enabled, COALESCE(priority,0) as priority FROM mail_filters ORDER BY priority ASC, created_at ASC").fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| {
                let id_str = row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("id").unwrap_or_default());
                let enabled_val = row.try_get::<bool,_>("enabled").map(|b| b).unwrap_or_else(|_| row.try_get::<i32,_>("enabled").map(|i| i!=0).unwrap_or(false));
                serde_json::json!({"id": id_str, "name": row.get::<String,_>("name"), "criteria": row.get::<String,_>("criteria_json"), "action": row.get::<String,_>("action_json"), "enabled": enabled_val, "priority": row.get::<i32,_>("priority")})
            }).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, name, criteria_json, action_json, enabled, COALESCE(priority,0) as priority FROM mail_filters ORDER BY priority ASC, created_at ASC").fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<String,_>("id"), "name": row.get::<String,_>("name"), "criteria": row.get::<String,_>("criteria_json"), "action": row.get::<String,_>("action_json"), "enabled": row.get::<i32,_>("enabled")!=0, "priority": row.get::<i32,_>("priority")})).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}
pub async fn create_filter(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let name = body.get("name").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let criteria = serde_json::to_string(body.get("criteria").unwrap_or(&serde_json::Value::Object(Default::default()))).unwrap();
    let action = serde_json::to_string(body.get("action").unwrap_or(&serde_json::Value::Object(Default::default()))).unwrap();
    let priority = body.get("priority").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let enabled = body.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
    let id = Uuid::new_v4();
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("INSERT INTO mail_filters (id, tenant_id, name, criteria_json, action_json, enabled, priority, created_at) VALUES ($1,'default',$2,$3,$4,$5,$6,NOW())").bind(id).bind(name).bind(&criteria).bind(&action).bind(enabled).bind(priority).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("INSERT INTO mail_filters (id, tenant_id, name, criteria_json, action_json, enabled, priority, created_at) VALUES (?,?,?,?,?,?,?,?)").bind(id.to_string()).bind("default").bind(name).bind(&criteria).bind(&action).bind(if enabled{1}else{0}).bind(priority).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({"success": true, "data": {"id": id.to_string()}}))))
}
pub async fn update_filter(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let priority = body.get("priority").and_then(|v| v.as_i64()).map(|v| v as i32);
    let enabled = body.get("enabled").and_then(|v| v.as_bool());
    let name = body.get("name").and_then(|v| v.as_str());
    match &state.db {
        DbPool::Postgres(pool) => {
            if let Some(p) = priority { sqlx::query("UPDATE mail_filters SET priority=$1 WHERE id=$2").bind(p).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(e) = enabled { sqlx::query("UPDATE mail_filters SET enabled=$1 WHERE id=$2").bind(e).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(n) = name { sqlx::query("UPDATE mail_filters SET name=$1 WHERE id=$2").bind(n).bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        }
        DbPool::Sqlite(pool) => {
            if let Some(p) = priority { sqlx::query("UPDATE mail_filters SET priority=? WHERE id=?").bind(p).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(e) = enabled { sqlx::query("UPDATE mail_filters SET enabled=? WHERE id=?").bind(if e{1}else{0}).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
            if let Some(n) = name { sqlx::query("UPDATE mail_filters SET name=? WHERE id=?").bind(n).bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}
pub async fn delete_filter(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("DELETE FROM mail_filters WHERE id=$1").bind(uid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("DELETE FROM mail_filters WHERE id=?").bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

// Vacation
pub async fn get_vacation(State(state): State<Arc<AppState>>, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = q.get("mailbox_id").and_then(|v| v.as_str()).unwrap_or("");
    let row: Option<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id, enabled, subject, body, start_at, end_at, interval_days FROM vacation_responders WHERE mailbox_id=$1::uuid LIMIT 1").bind(Uuid::parse_str(mailbox_id).unwrap_or(Uuid::nil())).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.map(|row| serde_json::json!({"id": row.get::<Uuid,_>("id").to_string(), "enabled": row.get::<bool,_>("enabled"), "subject": row.get::<String,_>("subject"), "body": row.get::<String,_>("body")}))
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, enabled, subject, body FROM vacation_responders WHERE mailbox_id=? LIMIT 1").bind(mailbox_id).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.map(|row| serde_json::json!({"id": row.get::<String,_>("id"), "enabled": row.get::<i32,_>("enabled")!=0, "subject": row.get::<String,_>("subject"), "body": row.get::<String,_>("body")}))
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": row.unwrap_or(serde_json::json!({"enabled": false}))})))
}
pub async fn set_vacation(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = body.get("mailbox_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let enabled = body.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    let subject = body.get("subject").and_then(|v| v.as_str()).unwrap_or("Out of office").to_string();
    let text = body.get("body").and_then(|v| v.as_str()).unwrap_or("I am out of office.").to_string();
    let id = Uuid::new_v4();
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO vacation_responders (id, mailbox_id, enabled, subject, body, interval_days, updated_at) VALUES ($1,$2,$3,$4,$5,1,NOW()) ON CONFLICT (mailbox_id) DO UPDATE SET enabled=$3, subject=$4, body=$5, updated_at=NOW()")
                .bind(id).bind(Uuid::parse_str(mailbox_id).unwrap_or(Uuid::nil())).bind(enabled).bind(&subject).bind(&text).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT OR REPLACE INTO vacation_responders (id, mailbox_id, enabled, subject, body, interval_days, updated_at) VALUES (?,?,?,?,?,?,?)")
                .bind(id.to_string()).bind(mailbox_id).bind(if enabled{1}else{0}).bind(&subject).bind(&text).bind(1).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

// Message Labels — attach/detach
pub async fn list_message_labels(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let mid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT l.id, l.name, l.color FROM mail_labels l JOIN message_labels ml ON ml.label_id=l.id WHERE ml.message_id=$1").bind(mid).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<Uuid,_>("id").to_string(), "name": row.get::<String,_>("name"), "color": row.get::<String,_>("color")})).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT l.id, l.name, l.color FROM mail_labels l JOIN message_labels ml ON ml.label_id=l.id WHERE ml.message_id=?").bind(mid.to_string()).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<String,_>("id"), "name": row.get::<String,_>("name"), "color": row.get::<String,_>("color")})).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}
pub async fn attach_label(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let mid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let label_id = body.get("label_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let lid = Uuid::parse_str(label_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("INSERT INTO message_labels (message_id, label_id) VALUES ($1,$2) ON CONFLICT DO NOTHING").bind(mid).bind(lid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("INSERT OR IGNORE INTO message_labels (message_id, label_id) VALUES (?,?)").bind(mid.to_string()).bind(lid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}
pub async fn detach_label(State(state): State<Arc<AppState>>, Path((id, label_id)): Path<(String, String)>) -> Result<Json<Value>, StatusCode> {
    let mid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let lid = Uuid::parse_str(&label_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("DELETE FROM message_labels WHERE message_id=$1 AND label_id=$2").bind(mid).bind(lid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        DbPool::Sqlite(pool) => { sqlx::query("DELETE FROM message_labels WHERE message_id=? AND label_id=?").bind(mid.to_string()).bind(lid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}
