use crate::api::{authz, AppState};
use aivory_mail_storage::db::DbPool;
use axum::{
    body::Body,
    extract::{Multipart, Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    Json,
};
use serde_json::Value;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

/// Max avatar upload: 2 MiB. Keeps ObjectStore + DB cheap and matches the
/// client-side check in settings/mail (both enforce the same limit so the
/// user gets an instant error before upload, not a 413 after).
const MAX_AVATAR_BYTES: usize = 2 * 1024 * 1024;

fn avatar_key(mailbox_id: &Uuid, content_type: &str) -> String {
    let ext = match content_type {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png",
    };
    format!("avatars/{}.{}", mailbox_id, ext)
}

fn detect_content_type(bytes: &[u8], declared: Option<&str>) -> Option<&'static str> {
    // Magic bytes win over the declared header — a renamed .exe must not
    // end up served as image/png from our domain.
    if bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        return Some("image/png");
    }
    if bytes.len() >= 3 && &bytes[0..3] == b"\xFF\xD8\xFF" {
        return Some("image/jpeg");
    }
    if bytes.len() >= 6 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    match declared {
        Some("image/png") => Some("image/png"),
        Some("image/jpeg") | Some("image/jpg") => Some("image/jpeg"),
        Some("image/gif") => Some("image/gif"),
        Some("image/webp") => Some("image/webp"),
        _ => None,
    }
}

/// Resolve the caller's own mailbox. Admins may pass ?mailbox_id= to manage
/// another mailbox (same convention as settings.rs via mailbox_scope);
/// ordinary users are always constrained to their JWT mailbox.
async fn own_mailbox(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    requested: Option<&str>,
) -> Result<Uuid, StatusCode> {
    authz::mailbox_scope(state, headers, requested)
        .await?
        .ok_or(StatusCode::BAD_REQUEST)
}

async fn read_avatar_meta(
    db: &DbPool,
    mailbox_id: &Uuid,
) -> Result<(String, Option<String>, Option<String>, Option<String>), StatusCode> {
    match db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query(
                "SELECT id, address, display_name, avatar_content_type, avatar_updated_at FROM mailboxes WHERE id=$1",
            )
            .bind(*mailbox_id)
            .fetch_optional(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
            Ok((
                row.get::<Uuid, _>("id").to_string(),
                row.get("address"),
                row.get("display_name"),
                row.get("avatar_content_type"),
            ))
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT id, address, display_name, avatar_content_type, avatar_updated_at FROM mailboxes WHERE id=?",
            )
            .bind(mailbox_id.to_string())
            .fetch_optional(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
            Ok((
                row.get("id"),
                row.get("address"),
                row.try_get("display_name").unwrap_or(None),
                row.try_get("avatar_content_type").unwrap_or(None),
            ))
        }
    }
}

/// GET /v1/me/profile — display name + avatar state for the settings Profile
/// tab and the inbox header. `avatar_url` is null when no avatar is set;
/// the client appends `?v=<updated_at>` itself for cache-busting.
pub async fn get_profile(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<Value>,
) -> Result<Json<Value>, StatusCode> {
    let requested = q.get("mailbox_id").and_then(|v| v.as_str());
    let mailbox_id = own_mailbox(&state, &headers, requested).await?;
    let email = authz::authenticated_email(&state, &headers)?;
    let (id, address, display_name, avatar_ct) = read_avatar_meta(&state.db, &mailbox_id).await?;
    let has_avatar = avatar_ct.is_some();
    let avatar_url: Option<String> = if has_avatar {
        Some(format!("/v1/me/avatar?mailbox_id={}", mailbox_id))
    } else {
        None
    };
    Ok(Json(serde_json::json!({"success": true, "data": {
        "email": email,
        "mailbox_id": id,
        "address": address,
        "display_name": display_name,
        "has_avatar": has_avatar,
        "avatar_content_type": avatar_ct,
        "avatar_url": avatar_url,
    }})))
}

/// PUT /v1/me/profile {display_name} — rename only (avatar goes through the
/// multipart endpoint below). Empty string clears back to null.
pub async fn update_profile(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let requested = body.get("mailbox_id").and_then(|v| v.as_str());
    let mailbox_id = own_mailbox(&state, &headers, requested).await?;
    let name = body
        .get("display_name")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let name = name.trim();
    if name.len() > 120 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let value: Option<String> = if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    };
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE mailboxes SET display_name=$1 WHERE id=$2")
                .bind(&value)
                .bind(mailbox_id)
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE mailboxes SET display_name=? WHERE id=?")
                .bind(&value)
                .bind(mailbox_id.to_string())
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(Json(
        serde_json::json!({"success": true, "data": {"display_name": value}}),
    ))
}

/// POST /v1/me/avatar (multipart field `avatar`) — validate magic bytes +
/// size, store to ObjectStore, record content type on the mailbox row.
/// Old extension variants for the same mailbox are removed so a png→jpg
/// switch doesn't leave a stale file behind.
pub async fn upload_avatar(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<Value>,
    mut multipart: Multipart,
) -> Result<Json<Value>, StatusCode> {
    let requested = q.get("mailbox_id").and_then(|v| v.as_str());
    let mailbox_id = own_mailbox(&state, &headers, requested).await?;
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut declared_ct: Option<String> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        let name = field.name().unwrap_or("").to_string();
        if name != "avatar" {
            continue;
        }
        declared_ct = field.content_type().map(|s| s.to_string());
        let bytes = field
            .bytes()
            .await
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        file_bytes = Some(bytes.to_vec());
    }
    let bytes = file_bytes.ok_or(StatusCode::BAD_REQUEST)?;
    if bytes.is_empty() || bytes.len() > MAX_AVATAR_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let content_type =
        detect_content_type(&bytes, declared_ct.as_deref()).ok_or(StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
    let key = avatar_key(&mailbox_id, content_type);
    state
        .store
        .put(&key, bytes, content_type)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // Remove stale sibling extensions (png→jpg switch etc.).
    for ext in ["png", "jpg", "gif", "webp"] {
        let sibling = format!("avatars/{}.{}", mailbox_id, ext);
        if sibling != key {
            let _ = state.store.delete(&sibling).await;
        }
    }
    let now = chrono::Utc::now().to_rfc3339();
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE mailboxes SET avatar_content_type=$1, avatar_updated_at=$2 WHERE id=$3")
                .bind(content_type)
                .bind(&now)
                .bind(mailbox_id)
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE mailboxes SET avatar_content_type=?, avatar_updated_at=? WHERE id=?")
                .bind(content_type)
                .bind(&now)
                .bind(mailbox_id.to_string())
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(Json(serde_json::json!({"success": true, "data": {
        "avatar_url": format!("/v1/me/avatar?mailbox_id={}", mailbox_id),
        "avatar_content_type": content_type,
        "updated_at": now,
    }})))
}

/// GET /v1/me/avatar — serve bytes. Accepts the session JWT as `?token=`
/// like attachments do, because <img> tags can't send Authorization.
pub async fn get_avatar(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Response<Body>, StatusCode> {
    let mut headers = headers;
    if !headers.contains_key(axum::http::header::AUTHORIZATION) {
        if let Some(t) = params.get("token").and_then(|v| v.as_str()) {
            if !t.is_empty() {
                if let Ok(v) =
                    axum::http::HeaderValue::from_str(&format!("Bearer {}", t))
                {
                    headers.insert(axum::http::header::AUTHORIZATION, v);
                }
            }
        }
    }
    let requested = params.get("mailbox_id").and_then(|v| v.as_str());
    // Avatar images are semi-public within the instance (message lists show
    // sender pictures, Gmail-workspace style): any authenticated user may
    // fetch any mailbox's avatar, but anonymous requests still 401 here.
    authz::authenticated_email(&state, &headers)?;
    let target: Uuid = if let Some(req) = requested {
        Uuid::parse_str(req).map_err(|_| StatusCode::BAD_REQUEST)?
    } else {
        authz::mailbox_scope(&state, &headers, None)
            .await?
            .ok_or(StatusCode::BAD_REQUEST)?
    };
    let (_, _, _, avatar_ct) = read_avatar_meta(&state.db, &target).await?;
    let content_type = avatar_ct.ok_or(StatusCode::NOT_FOUND)?;
    let key = avatar_key(&target, &content_type);
    let data = state
        .store
        .get(&key)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let mut resp = Response::new(Body::from(data));
    resp.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        content_type.parse().unwrap(),
    );
    resp.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        "private, max-age=3600".parse().unwrap(),
    );
    Ok(resp)
}

/// DELETE /v1/me/avatar — remove file + clear metadata.
pub async fn delete_avatar(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<Value>,
) -> Result<Json<Value>, StatusCode> {
    let requested = q.get("mailbox_id").and_then(|v| v.as_str());
    let mailbox_id = own_mailbox(&state, &headers, requested).await?;
    let (_, _, _, avatar_ct) = read_avatar_meta(&state.db, &mailbox_id).await?;
    if let Some(ct) = avatar_ct {
        let key = avatar_key(&mailbox_id, &ct);
        let _ = state.store.delete(&key).await;
    }
    for ext in ["png", "jpg", "gif", "webp"] {
        let _ = state
            .store
            .delete(&format!("avatars/{}.{}", mailbox_id, ext))
            .await;
    }
    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE mailboxes SET avatar_content_type=NULL, avatar_updated_at=NULL WHERE id=$1")
                .bind(mailbox_id)
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE mailboxes SET avatar_content_type=NULL, avatar_updated_at=NULL WHERE id=?")
                .bind(mailbox_id.to_string())
                .execute(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

/// GET /v1/avatars?emails=a@x,b@y — batch sender→avatar lookup for message
/// lists. Any authenticated user may call it (avatars are semi-public, see
/// get_avatar); only instance mailboxes are ever returned, everyone else
/// simply has no entry and the client keeps showing initials.
pub async fn avatars_by_email(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, StatusCode> {
    authz::authenticated_email(&state, &headers)?;
    let raw = params.get("emails").and_then(|v| v.as_str()).unwrap_or("");
    let mut emails: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty() && s.contains('@') && s.len() <= 320)
        .collect();
    emails.sort();
    emails.dedup();
    emails.truncate(100);
    if emails.is_empty() {
        return Ok(Json(serde_json::json!({"success": true, "data": {}})));
    }
    let mut out = serde_json::Map::new();
    match &state.db {
        DbPool::Postgres(pool) => {
            let rows = sqlx::query(
                "SELECT id, address, avatar_content_type FROM mailboxes WHERE lower(address) = ANY($1)",
            )
            .bind(&emails)
            .fetch_all(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            for row in rows {
                let id = row
                    .try_get::<uuid::Uuid, _>("id")
                    .map(|u| u.to_string())
                    .unwrap_or_else(|_| row.try_get::<String, _>("id").unwrap_or_default());
                let addr: String = row.get("address");
                let has_avatar = row
                    .try_get::<Option<String>, _>("avatar_content_type")
                    .unwrap_or(None)
                    .is_some();
                if !has_avatar {
                    continue;
                }
                out.insert(
                    addr.to_lowercase(),
                    serde_json::json!({"mailbox_id": id, "has_avatar": true, "avatar_url": format!("/v1/me/avatar?mailbox_id={}", id)}),
                );
            }
        }
        DbPool::Sqlite(pool) => {
            let placeholders = emails.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let q = format!(
                "SELECT id, address, avatar_content_type FROM mailboxes WHERE lower(address) IN ({})",
                placeholders
            );
            let mut query = sqlx::query(&q);
            for e in &emails {
                query = query.bind(e);
            }
            let rows = query
                .fetch_all(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            for row in rows {
                let id: String = row.get("id");
                let addr: String = row.get("address");
                let has_avatar: bool = row
                    .try_get::<Option<String>, _>("avatar_content_type")
                    .unwrap_or(None)
                    .is_some();
                if !has_avatar {
                    continue;
                }
                out.insert(
                    addr.to_lowercase(),
                    serde_json::json!({"mailbox_id": id, "has_avatar": true, "avatar_url": format!("/v1/me/avatar?mailbox_id={}", id)}),
                );
            }
        }
    }
    Ok(Json(serde_json::json!({"success": true, "data": out})))
}
