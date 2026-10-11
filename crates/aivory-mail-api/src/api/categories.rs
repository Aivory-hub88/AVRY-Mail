//! Inbox tabs (Primary / Promotions / Social / Updates / Forums).
//!
//! The rules live in `aivory_mail_core::categories`; this module stores the
//! result on each message, remembers a user's per-sender choice, and serves
//! the tab counts, the "move to tab" action and the backfill of old mail.

use std::sync::Arc;

use aivory_mail_core::categories::{self, Category};
use aivory_mail_storage::db::DbPool;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::api::{authz, AppState};

const DEFAULT_TABS: &str = "Primary,Promotions,Social";
const BACKFILL_BATCH: i64 = 500;

fn bare_sender(from: &str) -> String {
    let from = from.trim();
    let inner = match (from.rfind('<'), from.rfind('>')) {
        (Some(start), Some(end)) if start < end => &from[start + 1..end],
        _ => from,
    };
    inner.trim().to_lowercase()
}

/// The user's per-sender tab choice, if they made one.
pub async fn sender_lock(state: &Arc<AppState>, mailbox_id: &Uuid, from: &str) -> Option<Category> {
    let sender = bare_sender(from);
    let value: Option<String> = match &state.db {
        DbPool::Postgres(pool) => sqlx::query_scalar("SELECT category FROM sender_categories WHERE mailbox_id=$1 AND sender=$2")
            .bind(mailbox_id.to_string())
            .bind(&sender)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten(),
        DbPool::Sqlite(pool) => sqlx::query_scalar("SELECT category FROM sender_categories WHERE mailbox_id=? AND sender=?")
            .bind(mailbox_id.to_string())
            .bind(&sender)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten(),
    };
    value.as_deref().and_then(Category::parse)
}

/// Tab for a newly arrived message. Never fails: anything unexpected is
/// Primary, so a broken lookup can't hide mail.
pub async fn categorize_inbound(
    state: &Arc<AppState>,
    mailbox_id: &Uuid,
    from: &str,
    subject: &str,
    headers: &[(String, String)],
) -> Category {
    let lock = sender_lock(state, mailbox_id, from).await;
    categories::decide(lock, categories::classify(from, subject, headers)).category
}

/// Tabs this mailbox shows (`inbox.categories` setting; Primary always on).
pub async fn enabled_tabs(state: &Arc<AppState>, mailbox_id: &str) -> Vec<Category> {
    // A mailbox-specific value wins over the instance-wide one (NULL mailbox).
    let value: Option<String> = match &state.db {
        DbPool::Postgres(pool) => sqlx::query_scalar("SELECT value FROM user_settings WHERE category='inbox' AND key='categories' AND (mailbox_id=$1 OR mailbox_id IS NULL) ORDER BY (mailbox_id IS NULL) LIMIT 1")
            .bind(mailbox_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten(),
        DbPool::Sqlite(pool) => sqlx::query_scalar("SELECT value FROM user_settings WHERE category='inbox' AND key='categories' AND (mailbox_id=? OR mailbox_id IS NULL OR mailbox_id='') ORDER BY (mailbox_id IS NULL OR mailbox_id='') LIMIT 1")
            .bind(mailbox_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten(),
    };
    categories::enabled_from_setting(value.as_deref().unwrap_or(DEFAULT_TABS))
}

/// SQL predicate selecting the rows a tab lists. `category_expr` is the SQL
/// for the category value (a column, or a subquery for threads). Category
/// names come from a fixed enum, never from the request, so inlining them is
/// safe. A NULL category (mail from before tabs existed) counts as Primary.
pub fn tab_predicate(category_expr: &str, tab: Category, enabled: &[Category]) -> String {
    let stored = categories::stored_categories_for_tab(tab, enabled);
    if stored.is_empty() {
        return "1=0".to_string();
    }
    let list = stored
        .iter()
        .map(|c| format!("'{}'", c.as_str()))
        .collect::<Vec<_>>()
        .join(",");
    format!("COALESCE({},'primary') IN ({})", category_expr, list)
}

/// SQL for a thread's tab: the category of its newest Inbox message.
pub const THREAD_CATEGORY_EXPR: &str = "(SELECT tc.category FROM messages tc WHERE tc.thread_id=t.id AND tc.mailbox_id=t.mailbox_id AND tc.folder='Inbox' ORDER BY tc.created_at DESC LIMIT 1)";

/// `?category=` on an Inbox listing, resolved to a SQL predicate. `None` when
/// the request isn't for a tab (other folders, admin cross-mailbox views).
pub async fn tab_filter(
    state: &Arc<AppState>,
    params: &Value,
    folder: &str,
    mailbox_id: Option<&str>,
    category_expr: &str,
) -> Result<Option<String>, StatusCode> {
    let Some(raw) = params.get("category").and_then(Value::as_str).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let tab = Category::parse(raw).ok_or(StatusCode::BAD_REQUEST)?;
    let Some(mailbox_id) = mailbox_id else { return Ok(None) };
    if !folder.eq_ignore_ascii_case("Inbox") {
        return Ok(None);
    }
    let enabled = enabled_tabs(state, mailbox_id).await;
    Ok(Some(tab_predicate(category_expr, tab, &enabled)))
}

/// GET /v1/categories — enabled tabs with their unread Inbox counts.
pub async fn summary(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, StatusCode> {
    let mailbox = authz::mailbox_scope(&state, &headers, params.get("mailbox_id").and_then(Value::as_str))
        .await?
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_string();
    let enabled = enabled_tabs(&state, &mailbox).await;
    let rows: Vec<(String, i64)> = match &state.db {
        DbPool::Postgres(pool) => sqlx::query("SELECT COALESCE(category,'primary') AS c, COUNT(*) AS n FROM messages WHERE mailbox_id=$1 AND folder='Inbox' AND is_read=false AND (snoozed_until IS NULL OR snoozed_until <= NOW()) GROUP BY 1")
            .bind(Uuid::parse_str(&mailbox).map_err(|_| StatusCode::BAD_REQUEST)?)
            .fetch_all(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .into_iter()
            .map(|r| (r.get("c"), r.get("n")))
            .collect(),
        DbPool::Sqlite(pool) => sqlx::query("SELECT COALESCE(category,'primary') AS c, COUNT(*) AS n FROM messages WHERE mailbox_id=? AND folder='Inbox' AND is_read=0 AND (snoozed_until IS NULL OR snoozed_until='' OR datetime(snoozed_until) <= datetime('now')) GROUP BY 1")
            .bind(&mailbox)
            .fetch_all(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .into_iter()
            .map(|r| (r.get("c"), r.get("n")))
            .collect(),
    };
    let tabs: Vec<Value> = enabled
        .iter()
        .map(|tab| {
            let stored = categories::stored_categories_for_tab(*tab, &enabled);
            let unread: i64 = rows
                .iter()
                .filter(|(c, _)| Category::parse(c).is_some_and(|c| stored.contains(&c)))
                .map(|(_, n)| n)
                .sum();
            serde_json::json!({"id": tab.as_str(), "unread": unread})
        })
        .collect();
    Ok(Json(serde_json::json!({"success": true, "data": {"tabs": tabs}})))
}

/// POST /v1/messages/:id/category `{category, apply_to_sender?}` — move the
/// message's whole conversation to a tab. With `apply_to_sender` (default
/// true) the sender is locked there: their future mail and their other Inbox
/// conversations follow.
pub async fn set_for_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let message_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    authz::require_message_access(&state, &headers, message_id).await?;
    let category = body
        .get("category")
        .and_then(Value::as_str)
        .and_then(Category::parse)
        .ok_or(StatusCode::BAD_REQUEST)?;
    let apply_to_sender = body.get("apply_to_sender").and_then(Value::as_bool).unwrap_or(true);

    let (mailbox_id, thread_id, from_addr): (String, Option<String>, String) = match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT mailbox_id::text AS mailbox_id, thread_id::text AS thread_id, from_addr FROM messages WHERE id=$1")
                .bind(message_id)
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::NOT_FOUND)?;
            (r.get("mailbox_id"), r.get("thread_id"), r.get("from_addr"))
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT mailbox_id, thread_id, from_addr FROM messages WHERE id=?")
                .bind(message_id.to_string())
                .fetch_optional(pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::NOT_FOUND)?;
            (r.get("mailbox_id"), r.get("thread_id"), r.get("from_addr"))
        }
    };
    let sender = bare_sender(&from_addr);
    let cat = category.as_str();
    let now = chrono::Utc::now().to_rfc3339();
    let fail = |_| StatusCode::INTERNAL_SERVER_ERROR;

    let mut moved: u64 = 0;
    match &state.db {
        DbPool::Postgres(pool) => {
            let mb = Uuid::parse_str(&mailbox_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            moved += match &thread_id {
                Some(t) => sqlx::query("UPDATE messages SET category=$1 WHERE mailbox_id=$2 AND thread_id=$3::uuid")
                    .bind(cat).bind(mb).bind(t).execute(pool).await.map_err(fail)?.rows_affected(),
                None => sqlx::query("UPDATE messages SET category=$1 WHERE id=$2")
                    .bind(cat).bind(message_id).execute(pool).await.map_err(fail)?.rows_affected(),
            };
            if apply_to_sender && !sender.is_empty() {
                sqlx::query("INSERT INTO sender_categories (mailbox_id, sender, category, updated_at) VALUES ($1,$2,$3,$4) ON CONFLICT (mailbox_id, sender) DO UPDATE SET category=$3, updated_at=$4")
                    .bind(&mailbox_id).bind(&sender).bind(cat).bind(&now).execute(pool).await.map_err(fail)?;
                moved += sqlx::query("UPDATE messages SET category=$1 WHERE mailbox_id=$2 AND folder='Inbox' AND (lower(from_addr)=$3 OR lower(from_addr) LIKE $4)")
                    .bind(cat).bind(mb).bind(&sender).bind(format!("%<{}>", sender)).execute(pool).await.map_err(fail)?.rows_affected();
            }
        }
        DbPool::Sqlite(pool) => {
            moved += match &thread_id {
                Some(t) => sqlx::query("UPDATE messages SET category=? WHERE mailbox_id=? AND thread_id=?")
                    .bind(cat).bind(&mailbox_id).bind(t).execute(pool).await.map_err(fail)?.rows_affected(),
                None => sqlx::query("UPDATE messages SET category=? WHERE id=?")
                    .bind(cat).bind(message_id.to_string()).execute(pool).await.map_err(fail)?.rows_affected(),
            };
            if apply_to_sender && !sender.is_empty() {
                sqlx::query("INSERT INTO sender_categories (mailbox_id, sender, category, updated_at) VALUES (?,?,?,?) ON CONFLICT (mailbox_id, sender) DO UPDATE SET category=excluded.category, updated_at=excluded.updated_at")
                    .bind(&mailbox_id).bind(&sender).bind(cat).bind(&now).execute(pool).await.map_err(fail)?;
                moved += sqlx::query("UPDATE messages SET category=? WHERE mailbox_id=? AND folder='Inbox' AND (lower(from_addr)=? OR lower(from_addr) LIKE ?)")
                    .bind(cat).bind(&mailbox_id).bind(&sender).bind(format!("%<{}>", sender)).execute(pool).await.map_err(fail)?.rows_affected();
            }
        }
    }
    Ok(Json(serde_json::json!({"success": true, "data": {"category": cat, "sender_locked": apply_to_sender, "updated": moved}})))
}

/// POST /v1/categories/backfill — classify Inbox mail that arrived before tabs
/// existed. Works in batches; call again while `remaining > 0`.
pub async fn backfill(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, StatusCode> {
    let mailbox = authz::mailbox_scope(&state, &headers, params.get("mailbox_id").and_then(Value::as_str))
        .await?
        .ok_or(StatusCode::BAD_REQUEST)?;
    let fail = |_| StatusCode::INTERNAL_SERVER_ERROR;
    let rows: Vec<(String, String, Option<String>, Option<String>)> = match &state.db {
        DbPool::Postgres(pool) => sqlx::query("SELECT id::text AS id, from_addr, subject, headers_json::text AS headers FROM messages WHERE mailbox_id=$1 AND folder='Inbox' AND category IS NULL ORDER BY created_at DESC LIMIT $2")
            .bind(mailbox).bind(BACKFILL_BATCH).fetch_all(pool).await.map_err(fail)?
            .into_iter().map(|r| (r.get("id"), r.get("from_addr"), r.get("subject"), r.get("headers"))).collect(),
        DbPool::Sqlite(pool) => sqlx::query("SELECT id, from_addr, subject, headers_json AS headers FROM messages WHERE mailbox_id=? AND folder='Inbox' AND category IS NULL ORDER BY created_at DESC LIMIT ?")
            .bind(mailbox.to_string()).bind(BACKFILL_BATCH).fetch_all(pool).await.map_err(fail)?
            .into_iter().map(|r| (r.get("id"), r.get("from_addr"), r.get("subject"), r.get("headers"))).collect(),
    };
    let mut counts: std::collections::BTreeMap<&'static str, u64> = Default::default();
    for (id, from, subject, headers_raw) in &rows {
        let headers_value: Value = headers_raw.as_deref().and_then(|h| serde_json::from_str(h).ok()).unwrap_or(Value::Null);
        let header_pairs = categories::headers_from_json(&headers_value);
        let cat = categorize_inbound(&state, &mailbox, from, subject.as_deref().unwrap_or(""), &header_pairs).await;
        match &state.db {
            DbPool::Postgres(pool) => sqlx::query("UPDATE messages SET category=$1 WHERE id=$2::uuid")
                .bind(cat.as_str()).bind(id).execute(pool).await.map_err(fail)?.rows_affected(),
            DbPool::Sqlite(pool) => sqlx::query("UPDATE messages SET category=? WHERE id=?")
                .bind(cat.as_str()).bind(id).execute(pool).await.map_err(fail)?.rows_affected(),
        };
        *counts.entry(cat.as_str()).or_default() += 1;
    }
    let remaining: i64 = match &state.db {
        DbPool::Postgres(pool) => sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE mailbox_id=$1 AND folder='Inbox' AND category IS NULL")
            .bind(mailbox).fetch_one(pool).await.map_err(fail)?,
        DbPool::Sqlite(pool) => sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE mailbox_id=? AND folder='Inbox' AND category IS NULL")
            .bind(mailbox.to_string()).fetch_one(pool).await.map_err(fail)?,
    };
    Ok(Json(serde_json::json!({"success": true, "data": {"classified": rows.len(), "by_category": counts, "remaining": remaining}})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sender_is_bare_lowercase_address() {
        assert_eq!(bare_sender("Shop <Hello@Shop.Example>"), "hello@shop.example");
        assert_eq!(bare_sender(" a@b.c "), "a@b.c");
    }

    #[test]
    fn predicate_lists_only_enum_values() {
        let enabled = vec![Category::Primary, Category::Promotions];
        assert_eq!(
            tab_predicate("m.category", Category::Primary, &enabled),
            "COALESCE(m.category,'primary') IN ('primary','social','updates','forums')"
        );
        assert_eq!(tab_predicate("category", Category::Promotions, &enabled), "COALESCE(category,'primary') IN ('promotions')");
        assert_eq!(tab_predicate("category", Category::Social, &enabled), "1=0");
    }
}
