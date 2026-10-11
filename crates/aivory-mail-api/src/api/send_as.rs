use std::sync::Arc;
use axum::{extract::{State, Path, Query}, Json, http::{HeaderMap, StatusCode}};
use serde_json::Value;
use uuid::Uuid;
use sqlx::Row;
use crate::api::{authz, AppState};
use aivory_mail_storage::db::DbPool;
use aivory_mail_core::validation;

pub async fn list(State(state): State<Arc<AppState>>, headers: HeaderMap, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    // Ordinary users only ever see their own mailbox's aliases; admins may
    // ask for any mailbox, or all of them.
    let scope = authz::mailbox_scope(&state, &headers, q.get("mailbox_id").and_then(|v| v.as_str())).await?;
    let scope = scope.map(|id| id.to_string());
    let mailbox_id = scope.as_deref();
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = if let Some(mid) = mailbox_id {
                // mailbox_id is TEXT in the live schema — validate it looks
                // like a UUID, but bind the string itself; binding the Uuid
                // type here 500'd every call ("operator does not exist:
                // text = uuid").
                Uuid::parse_str(mid).map_err(|_| StatusCode::BAD_REQUEST)?;
                sqlx::query("SELECT id, mailbox_id, alias_email, display_name, is_default FROM send_as_aliases WHERE mailbox_id=$1 ORDER BY is_default DESC, created_at DESC").bind(mid).fetch_all(pool).await
            } else {
                sqlx::query("SELECT id, mailbox_id, alias_email, display_name, is_default FROM send_as_aliases ORDER BY created_at DESC").fetch_all(pool).await
            };
            let r = r.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({"id": row.try_get::<Uuid,_>("id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("id").unwrap_or_default()), "mailbox_id": row.try_get::<Uuid,_>("mailbox_id").map(|u| u.to_string()).unwrap_or_else(|_| row.try_get::<String,_>("mailbox_id").unwrap_or_default()), "alias_email": row.get::<String,_>("alias_email"), "display_name": row.get::<String,_>("display_name"), "is_default": row.try_get::<bool,_>("is_default").unwrap_or_else(|_| row.try_get::<i32,_>("is_default").map(|i| i!=0).unwrap_or(false))})).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = if let Some(mid) = mailbox_id {
                sqlx::query("SELECT id, mailbox_id, alias_email, display_name, is_default FROM send_as_aliases WHERE mailbox_id=? ORDER BY is_default DESC").bind(mid).fetch_all(pool).await
            } else {
                sqlx::query("SELECT id, mailbox_id, alias_email, display_name, is_default FROM send_as_aliases ORDER BY created_at DESC").fetch_all(pool).await
            };
            let r = r.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            r.into_iter().map(|row| serde_json::json!({"id": row.get::<String,_>("id"), "mailbox_id": row.get::<String,_>("mailbox_id"), "alias_email": row.get::<String,_>("alias_email"), "display_name": row.get::<String,_>("display_name"), "is_default": row.get::<i32,_>("is_default")!=0})).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

pub async fn create(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<Value>) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let mailbox_id_str = body.get("mailbox_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let mailbox_id = Uuid::parse_str(mailbox_id_str).map_err(|_| StatusCode::BAD_REQUEST)?;
    // Non-admins can only add aliases to their own mailbox.
    authz::mailbox_scope(&state, &headers, Some(mailbox_id_str)).await?;
    let alias_email = body.get("alias_email").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    validation::validate_email(alias_email).map_err(|_| StatusCode::BAD_REQUEST)?;
    let alias_email = validation::normalize_email(alias_email);
    let caller = authz::authenticated_email(&state, &headers)?;
    if !authz::is_admin(&state, &caller).await && !user_may_claim_alias(&state, mailbox_id, &alias_email).await? {
        // A Send As alias both sends as the address and receives its mail, so
        // an address that already belongs to someone else is never claimable.
        return Ok((StatusCode::FORBIDDEN, Json(serde_json::json!({"success": false, "error": "This address already belongs to another mailbox."}))));
    }
    let display_name = body.get("display_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let is_default = body.get("is_default").and_then(|v| v.as_bool()).unwrap_or(false);
    // Reject exact duplicates with a readable message instead of a bare 409:
    // without this the admin aliases tab accumulated identical rows (same
    // alias twice on one mailbox) with no way to tell them apart.
    let dupe: i64 = match &state.db {
        DbPool::Postgres(pool) => sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM send_as_aliases WHERE mailbox_id=$1 AND lower(alias_email)=lower($2)")
            .bind(mailbox_id.to_string()).bind(&alias_email).fetch_one(pool).await.unwrap_or(0),
        DbPool::Sqlite(pool) => sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM send_as_aliases WHERE mailbox_id=? AND lower(alias_email)=lower(?)")
            .bind(mailbox_id.to_string()).bind(&alias_email).fetch_one(pool).await.unwrap_or(0),
    };
    if dupe > 0 {
        return Ok((StatusCode::CONFLICT, Json(serde_json::json!({"success": false, "error": "Alias already exists for this mailbox"}))));
    }
    let id = Uuid::new_v4();
    if is_default {
        match &state.db {
            DbPool::Postgres(pool) => { let _ = sqlx::query("UPDATE send_as_aliases SET is_default=0 WHERE mailbox_id=$1").bind(mailbox_id.to_string()).execute(pool).await; }
            DbPool::Sqlite(pool) => { let _ = sqlx::query("UPDATE send_as_aliases SET is_default=0 WHERE mailbox_id=?").bind(mailbox_id.to_string()).execute(pool).await; }
        }
    }
    match &state.db {
        DbPool::Postgres(pool) => { sqlx::query("INSERT INTO send_as_aliases (id, mailbox_id, alias_email, display_name, is_default, created_at) VALUES ($1,$2,$3,$4,$5,NOW())").bind(id.to_string()).bind(mailbox_id.to_string()).bind(&alias_email).bind(&display_name).bind(if is_default{1}else{0}).execute(pool).await.map_err(|e| { tracing::error!("send_as insert: {}", e); StatusCode::CONFLICT })?; }
        DbPool::Sqlite(pool) => { sqlx::query("INSERT INTO send_as_aliases (id, mailbox_id, alias_email, display_name, is_default, created_at) VALUES (?,?,?,?,?,?)").bind(id.to_string()).bind(mailbox_id.to_string()).bind(&alias_email).bind(&display_name).bind(if is_default{1}else{0}).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await.map_err(|_| StatusCode::CONFLICT)?; }
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({"success": true, "data": {"id": id.to_string(), "mailbox_id": mailbox_id.to_string(), "alias_email": alias_email}}))))
}

pub async fn remove(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let uid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    // None = admin (any alias); Some(own) = only aliases of the caller's mailbox.
    let scope = authz::mailbox_scope(&state, &headers, None).await?.map(|m| m.to_string());
    match (&state.db, scope) {
        (DbPool::Postgres(pool), None) => { sqlx::query("DELETE FROM send_as_aliases WHERE id=$1").bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        (DbPool::Postgres(pool), Some(own)) => { sqlx::query("DELETE FROM send_as_aliases WHERE id=$1 AND mailbox_id=$2").bind(uid.to_string()).bind(own).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        (DbPool::Sqlite(pool), None) => { sqlx::query("DELETE FROM send_as_aliases WHERE id=?").bind(uid.to_string()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
        (DbPool::Sqlite(pool), Some(own)) => { sqlx::query("DELETE FROM send_as_aliases WHERE id=? AND mailbox_id=?").bind(uid.to_string()).bind(own).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(Json(serde_json::json!({"success": true})))
}

/// A non-admin may add any alias nobody else owns. Taken means inbound routing
/// already delivers that exact address to another mailbox — its own address,
/// an admin alias, `use_all_domains`, or someone else's Send As alias — so a
/// user can never take over a colleague's address. A domain catch-all is not
/// ownership of every address under it, so it doesn't block.
async fn user_may_claim_alias(state: &Arc<AppState>, own_mailbox: Uuid, alias: &str) -> Result<bool, StatusCode> {
    let routed = crate::mail::routing::resolve_recipient(state, alias)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(!taken_by_other(routed.accept, routed.mailbox_id, &routed.reason, own_mailbox))
}

fn taken_by_other(accept: bool, mailbox_id: Option<Uuid>, reason: &str, own_mailbox: Uuid) -> bool {
    accept && reason != "catch-all" && mailbox_id.is_some_and(|id| id != own_mailbox)
}

#[cfg(test)]
mod tests {
    use super::taken_by_other;
    use uuid::Uuid;

    #[test]
    fn only_another_mailbox_s_exact_address_blocks_an_alias() {
        let (me, other) = (Uuid::new_v4(), Uuid::new_v4());
        assert!(taken_by_other(true, Some(other), "mailbox match", me));
        assert!(taken_by_other(true, Some(other), "alias", me));
        assert!(taken_by_other(true, Some(other), "send-as alias", me));
        assert!(!taken_by_other(true, Some(me), "alias", me));
        assert!(!taken_by_other(true, Some(other), "catch-all", me));
        assert!(!taken_by_other(false, None, "no such mailbox", me));
    }
}
