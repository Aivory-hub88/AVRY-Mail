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
            let r = sqlx::query("SELECT id, name, email, description, created_at FROM groups WHERE tenant_id='default' ORDER BY created_at DESC").fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut out = Vec::new();
            for row in r {
                let gid: Uuid = row.get("id");
                let members: Vec<String> = sqlx::query_scalar("SELECT m.address FROM mailboxes m JOIN group_members gm ON gm.mailbox_id=m.id WHERE gm.group_id=$1").bind(gid).fetch_all(pool).await.unwrap_or_default();
                out.push(serde_json::json!({"id": gid.to_string(), "name": row.get::<String,_>("name"), "email": row.get::<String,_>("email"), "description": row.get::<String,_>("description"), "members": members, "created_at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339()}));
            }
            out
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id, name, email, description, created_at FROM groups WHERE tenant_id='default' ORDER BY created_at DESC").fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut out = Vec::new();
            for row in r {
                let gid: String = row.get("id");
                let members: Vec<String> = sqlx::query_scalar("SELECT m.address FROM mailboxes m JOIN group_members gm ON gm.mailbox_id=m.id WHERE gm.group_id=?").bind(&gid).fetch_all(pool).await.unwrap_or_default();
                out.push(serde_json::json!({"id": gid, "name": row.get::<String,_>("name"), "email": row.get::<String,_>("email"), "description": row.get::<String,_>("description"), "members": members, "created_at": row.get::<String,_>("created_at")}));
            }
            out
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn create(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let name = body.get("name").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let email = body.get("email").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let description = body.get("description").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() || email.is_empty() || !email.contains('@') { return Err(StatusCode::BAD_REQUEST); }
    let id = Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO groups (id, tenant_id, name, email, description, created_at) VALUES ($1,'default',$2,$3,$4,NOW())")
                .bind(id).bind(name).bind(email).bind(description).execute(pool).await.map_err(|_| StatusCode::BAD_REQUEST)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO groups (id, tenant_id, name, email, description, created_at) VALUES (?,?,?,?,?,?)")
                .bind(id.to_string()).bind("default").bind(name).bind(email).bind(description).bind(&now).execute(pool).await.map_err(|_| StatusCode::BAD_REQUEST)?;
        }
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({"success": true, "data": {"id": id.to_string()}}))))
}

pub async fn remove(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let gid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("DELETE FROM group_members WHERE group_id=$1").bind(gid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            sqlx::query("DELETE FROM groups WHERE id=$1").bind(gid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("DELETE FROM group_members WHERE group_id=?").bind(gid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            sqlx::query("DELETE FROM groups WHERE id=?").bind(gid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn add_member(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let gid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let mailbox_id = body.get("mailbox_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let mid = Uuid::parse_str(mailbox_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO group_members (group_id, mailbox_id) VALUES ($1,$2) ON CONFLICT DO NOTHING").bind(gid).bind(mid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT OR IGNORE INTO group_members (group_id, mailbox_id) VALUES (?,?)").bind(gid.to_string()).bind(mid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn remove_member(State(state): State<Arc<AppState>>, Path((id, member_id)): Path<(String, String)>) -> Result<Json<Value>, StatusCode> {
    let gid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let mid = Uuid::parse_str(&member_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("DELETE FROM group_members WHERE group_id=$1 AND mailbox_id=$2").bind(gid).bind(mid).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("DELETE FROM group_members WHERE group_id=? AND mailbox_id=?").bind(gid.to_string()).bind(mid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}
