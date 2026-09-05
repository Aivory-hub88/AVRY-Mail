use std::sync::Arc;
use axum::{Router, routing::{get, post, put, delete}, extract::{State, Query}, Json, http::StatusCode};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;
use crate::{config::Config, realtime::RealtimeHub};
use aivory_mail_storage::{db::DbPool, object_store::ObjectStore};

pub mod domains;
pub mod mailboxes;
pub mod messages;
pub mod send;
pub mod threads;
pub mod intelligence;
pub mod webhooks;
pub mod share;
pub mod signatures;
pub mod calendar;
pub mod calendar_events;
pub mod search;
pub mod cognee;
pub mod api_keys;
pub mod knowledge;
pub mod settings;
pub mod contacts;
pub mod folders;
pub mod audit;
pub mod internal;
pub mod send_as;
pub mod auth;
pub mod groups;
pub mod ai_chat;
pub mod webhooks_registry;
pub mod agent_tasks;
pub mod authz;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub db: DbPool,
    pub store: Arc<dyn ObjectStore>,
    pub hub: RealtimeHub,
}

/// Axum's `Query<serde_json::Value>` deserializes every query-string value
/// as a JSON string (there's no type info in a raw query string) — so
/// `params.get("per_page").and_then(|v| v.as_i64())` silently returns `None`
/// for a real request like `?per_page=1`, and every such call site quietly
/// fell back to its default instead of honoring what the client asked for.
/// This was the actual cause of a sidebar unread-count badge that never
/// reflected reality: a `per_page=1` "just get the count" fetch always came
/// back with the default page size instead. Route every page/per_page/limit
/// query param through this instead of a raw `.as_i64()`.
pub fn query_i64(v: Option<&Value>) -> Option<i64> {
    v.and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok())))
}

/// Endpoints that list or manage data across the whole instance rather than
/// a single mailbox: domain/mailbox provisioning, groups, API keys, the
/// audit log, and the webhook registry. These backed the admin console with
/// no server-side check at all — any request (even unauthenticated curl)
/// got the full mailbox list, could create/delete accounts, and could read
/// every domain's DKIM keys. Gated to whoever `authz::is_admin` recognizes:
/// a domain's own `admin_email`, or the instance-wide MAIL_ADMIN_EMAIL /
/// SUPERADMIN_EMAIL fallback.
fn admin_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/domains", get(domains::list).post(domains::create))
        .route("/v1/domains/:id", get(domains::get_one).delete(domains::remove))
        .route("/v1/domains/:id/verify", post(domains::verify))
        .route("/v1/domains/:id/dns", get(domains::dns_status))
        .route("/v1/domains/:id/dkim", get(domains::dkim_record))
        .route("/v1/mailboxes", get(mailboxes::list).post(mailboxes::create))
        .route("/v1/mailboxes/:id", get(mailboxes::get_one).put(mailboxes::update).delete(mailboxes::remove))
        .route("/v1/audit-logs", get(audit::list))
        .route("/v1/groups", get(groups::list).post(groups::create))
        .route("/v1/groups/:id", delete(groups::remove))
        .route("/v1/groups/:id/members", post(groups::add_member))
        .route("/v1/groups/:id/members/:member_id", delete(groups::remove_member))
        .route("/v1/api-keys", get(api_keys::list).post(api_keys::create))
        .route("/v1/api-keys/:id", delete(api_keys::remove))
        .route("/v1/mcp/generate-link", post(api_keys::generate_mcp_link))
        .route("/v1/webhooks", get(webhooks_registry::list).post(webhooks_registry::create))
        .route("/v1/webhooks/:id", delete(webhooks_registry::remove))
        .route("/v1/webhooks/:id/deliveries", get(webhooks_registry::deliveries))
        .route("/v1/webhooks/:id/retry", post(webhooks_registry::retry))
        .route_layer(axum::middleware::from_fn_with_state(state, authz::require_admin_mw))
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        // health
        .route("/health", get(health))
        .route("/v1/health", get(health))
        // auth — login before mail
        .route("/v1/auth/login", post(auth::login))
        .route("/v1/auth/me", get(auth::me))
        .merge(admin_router(state.clone()))
        // internal (protected by x-internal-token, used by the SMTP ingress)
        .route("/v1/internal/resolve-recipient", get(internal::resolve_recipient))
        // messages
        .route("/v1/messages", get(messages::list))
        .route("/v1/messages/:id", get(messages::get_one).delete(messages::remove))
        .route("/v1/messages/:id/read", put(messages::mark_read))
        .route("/v1/messages/:id/move", post(messages::move_message))
        .route("/v1/messages/:id/snooze", post(messages::snooze).delete(messages::unsnooze))
        .route("/v1/messages/:id/attachments/:att_id", get(messages::download_attachment))
        // threads
        .route("/v1/threads", get(threads::list))
        .route("/v1/threads/:id", get(threads::get_one))
        .route("/v1/threads/:id/reply", post(threads::reply))
        .route("/v1/threads/:id/crawl", get(threads::crawl))
        .route("/v1/threads/:id/follow-up", get(threads::follow_up).post(threads::follow_up))
        // send
        .route("/v1/send", post(send::send_email))
        .route("/v1/send/batch", post(send::send_batch))
        // intelligence
        .route("/v1/intelligence/analyze", post(intelligence::analyze))
        .route("/v1/intelligence/suggest", post(intelligence::suggest))
        .route("/v1/agent/actions", post(intelligence::agent_actions))
        // webhooks (inbound)
        .route("/v1/webhooks/inbound", post(webhooks::inbound))
        .route("/v1/webhooks/cloudflare", post(webhooks::cloudflare_email))
        // drafts
        .route("/v1/calendar/status", get(calendar::status))
        .route("/v1/calendar/event-types", get(calendar::event_types))
        .route("/v1/calendar/slots", get(calendar::slots))
        .route("/v1/calendar/bookings", post(calendar::create_booking))
        .route("/v1/calendar/propose", post(calendar::propose))
        .route("/v1/search", get(search::search))
        .route("/v1/inbox/overview", get(search::overview))
        .route("/v1/threads/:id/memory", get(search::memory))
        .route("/v1/calendar/events", get(calendar_events::list).post(calendar_events::create))
        .route("/v1/calendar/events/:id", axum::routing::put(calendar_events::update).delete(calendar_events::remove))
        .route("/v1/signatures", get(signatures::list).post(signatures::create))
        .route("/v1/signatures/:id", axum::routing::put(signatures::update).delete(signatures::remove))
        .route("/v1/drafts", get(share::list_drafts).post(share::save_draft))
        .route("/v1/cognee/sync", get(cognee::sync))
        .route("/v1/mcp/tools", get(cognee::mcp_tools))
        .route("/mcp", get(cognee::mcp_tools).post(crate::mcp::mcp_handler))
        .route("/v1/knowledge/compile", get(knowledge::compile))
        .route("/v1/settings", get(settings::get).post(settings::set))
        .route("/v1/labels", get(settings::list_labels).post(settings::create_label))
        .route("/v1/labels/:id", delete(settings::delete_label))
        .route("/v1/messages/:id/labels", get(settings::list_message_labels).post(settings::attach_label))
        .route("/v1/messages/:id/labels/:label_id", delete(settings::detach_label))
        .route("/v1/filters", get(settings::list_filters).post(settings::create_filter))
        .route("/v1/filters/:id", put(settings::update_filter).delete(settings::delete_filter))
        .route("/v1/vacation", get(settings::get_vacation).post(settings::set_vacation))
        .route("/v1/contacts", get(contacts::list))
        .route("/v1/contacts/block", post(contacts::block))
        .route("/v1/contacts/import", post(contacts::import_contacts))
        .route("/v1/folders", get(folders::list).post(folders::create))
        .route("/v1/folders/:id", delete(folders::remove))
        .route("/v1/send-as", get(send_as::list).post(send_as::create))
        .route("/v1/send-as/:id", delete(send_as::remove))
        .route("/v1/agent/tasks", get(agent_tasks::list).post(agent_tasks::create))
        .route("/v1/agent/tasks/:id", get(agent_tasks::get_one).put(agent_tasks::update))
        .route("/v1/messages/:id/star", post(share::toggle_star))
        // share
        .route("/v1/messages/:id/share", post(share::create_share))
        .route("/v1/share/:id", get(share::get_shared))
        // email assistant — zeroclaw vanilla sub-agent + mission control
        .route("/v1/ai/ask", post(ai_chat::ask))
        .route("/v1/ai/history", get(ai_chat::history))
        .route("/v1/ai/push-to-mission-control", post(ai_chat::push_to_mission_control))
        .route("/v1/notifications", get(ai_chat::list_notifications))
        // realtime
        .route("/v1/realtime/ws", get(crate::realtime_ws::ws_handler))
        .route("/v1/stats", get(stats))
        .with_state(state)
}

async fn health(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let db_ok = state.db.health_check().await.is_ok();
    Ok(Json(serde_json::json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "service": "aivory-mail",
        "version": env!("CARGO_PKG_VERSION"),
        "mode": state.config.mail_mode,
        "storage": state.config.storage_backend,
        "db": if db_ok { "connected" } else { "error" },
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

// mailbox_id is optional: the Admin Overview tab wants instance-wide counts
// (all mailboxes), while the per-user Inbox sidebar must pass its own
// mailbox_id — otherwise "by_folder" silently summed every mailbox on the
// instance and every account's sidebar showed the same mixed-together counts.
async fn stats(State(state): State<Arc<AppState>>, headers: axum::http::HeaderMap, Query(params): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = params.get("mailbox_id").and_then(|v| v.as_str());
    // Omitting mailbox_id returns instance-wide counts across every
    // mailbox — that's admin-console data, not something any logged-in (or
    // unauthenticated) caller should get by just leaving the param off.
    if mailbox_id.is_none() {
        authz::require_admin(&state, &headers).await?;
    }
    let (domains, mailboxes, messages, by_folder, snoozed) = match &state.db {
        DbPool::Postgres(pool) => {
            let d: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM domains").fetch_one(pool).await.unwrap_or(0);
            let m: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mailboxes").fetch_one(pool).await.unwrap_or(0);
            let (msg, rows, snoozed): (i64, Vec<sqlx::postgres::PgRow>, i64) = if let Some(mid) = mailbox_id {
                let uid = Uuid::parse_str(mid).map_err(|_| StatusCode::BAD_REQUEST)?;
                let msg = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE mailbox_id=$1").bind(uid).fetch_one(pool).await.unwrap_or(0);
                let rows = sqlx::query("SELECT folder, COUNT(*) as c FROM messages WHERE mailbox_id=$1 GROUP BY folder").bind(uid).fetch_all(pool).await.unwrap_or_default();
                let snoozed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE mailbox_id=$1 AND snoozed_until IS NOT NULL AND snoozed_until > NOW()").bind(uid).fetch_one(pool).await.unwrap_or(0);
                (msg, rows, snoozed)
            } else {
                let msg = sqlx::query_scalar("SELECT COUNT(*) FROM messages").fetch_one(pool).await.unwrap_or(0);
                let rows = sqlx::query("SELECT folder, COUNT(*) as c FROM messages GROUP BY folder").fetch_all(pool).await.unwrap_or_default();
                let snoozed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE snoozed_until IS NOT NULL AND snoozed_until > NOW()").fetch_one(pool).await.unwrap_or(0);
                (msg, rows, snoozed)
            };
            let mut map = serde_json::Map::new();
            for r in rows {
                let f: String = r.get("folder");
                let c: i64 = r.get("c");
                map.insert(f, serde_json::json!(c));
            }
            if snoozed>0 { map.insert("Snoozed".into(), serde_json::json!(snoozed)); }
            (d,m,msg, Value::Object(map), snoozed)
        }
        DbPool::Sqlite(pool) => {
            let d: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM domains").fetch_one(pool).await.unwrap_or(0);
            let m: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mailboxes").fetch_one(pool).await.unwrap_or(0);
            let (msg, rows, snoozed): (i64, Vec<sqlx::sqlite::SqliteRow>, i64) = if let Some(mid) = mailbox_id {
                let msg = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE mailbox_id=?").bind(mid).fetch_one(pool).await.unwrap_or(0);
                let rows = sqlx::query("SELECT folder, COUNT(*) as c FROM messages WHERE mailbox_id=? GROUP BY folder").bind(mid).fetch_all(pool).await.unwrap_or_default();
                let snoozed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE mailbox_id=? AND snoozed_until IS NOT NULL AND datetime(snoozed_until) > datetime('now')").bind(mid).fetch_one(pool).await.unwrap_or(0);
                (msg, rows, snoozed)
            } else {
                let msg = sqlx::query_scalar("SELECT COUNT(*) FROM messages").fetch_one(pool).await.unwrap_or(0);
                let rows = sqlx::query("SELECT folder, COUNT(*) as c FROM messages GROUP BY folder").fetch_all(pool).await.unwrap_or_default();
                let snoozed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE snoozed_until IS NOT NULL AND datetime(snoozed_until) > datetime('now')").fetch_one(pool).await.unwrap_or(0);
                (msg, rows, snoozed)
            };
            let mut map = serde_json::Map::new();
            for r in rows {
                let f: String = r.get("folder");
                let c: i64 = r.get("c");
                map.insert(f, serde_json::json!(c));
            }
            if snoozed>0 { map.insert("Snoozed".into(), serde_json::json!(snoozed)); }
            (d,m,msg, Value::Object(map), snoozed)
        }
    };
    Ok(Json(serde_json::json!({ "domains": domains, "mailboxes": mailboxes, "messages": messages, "by_folder": by_folder, "snoozed": snoozed })))
}
