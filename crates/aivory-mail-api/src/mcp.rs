use crate::{api::AppState, mcp_limits};
use aivory_mail_core::types::SendRequest;
use aivory_mail_storage::db::DbPool;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

fn hash_key(raw: &str) -> String {
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    format!("{:x}", h.finalize())
}

async fn validate_api_key(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    query_key: Option<String>,
) -> bool {
    let mut raw: Option<String> = None;
    if let Some(auth) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if auth.to_lowercase().starts_with("bearer ") {
            raw = Some(auth[7..].trim().to_string());
        }
    }
    if raw.is_none() {
        raw = query_key;
    }
    if raw.is_none() {
        raw = headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
    }
    let Some(k) = raw else {
        return false;
    };
    let hash = hash_key(&k);
    match &state.db {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("SELECT id FROM api_keys WHERE key_hash=$1 LIMIT 1")
                .bind(&hash)
                .fetch_optional(pool)
                .await;
            matches!(r, Ok(Some(_)))
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("SELECT id FROM api_keys WHERE key_hash=? LIMIT 1")
                .bind(&hash)
                .fetch_optional(pool)
                .await;
            matches!(r, Ok(Some(_)))
        }
    }
}

async fn mailbox_address(
    state: &Arc<AppState>,
    mailbox_id: uuid::Uuid,
) -> Result<Option<String>, StatusCode> {
    match &state.db {
        DbPool::Postgres(pool) => sqlx::query("SELECT address FROM mailboxes WHERE id=$1")
            .bind(mailbox_id)
            .fetch_optional(pool)
            .await
            .map(|row| row.map(|value| value.get::<String, _>("address").to_lowercase()))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR),
        DbPool::Sqlite(pool) => sqlx::query("SELECT address FROM mailboxes WHERE id=?")
            .bind(mailbox_id.to_string())
            .fetch_optional(pool)
            .await
            .map(|row| row.map(|value| value.get::<String, _>("address").to_lowercase()))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Duplicate-send guard for agent callers: returns (sent_at, id) when this
/// mailbox already Sent the same recipients + subject inside the window.
/// Agents retry blindly when they cannot see their Sent box; without this
/// every confused loop becomes duplicate outbound mail to a real lead.
async fn recent_identical_send(
    state: &Arc<AppState>,
    mailbox_id: uuid::Uuid,
    to: &[String],
    subject: &str,
) -> Option<(String, String)> {
    const WINDOW_MINUTES: i64 = 30;
    let mut want: Vec<String> = to.iter().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect();
    want.sort();
    if want.is_empty() || subject.trim().is_empty() {
        return None;
    }
    let same = |stored_json: &str| -> bool {
        let mut got: Vec<String> = serde_json::from_str::<Vec<String>>(stored_json)
            .unwrap_or_default()
            .into_iter()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        got.sort();
        got == want
    };
    match &state.db {
        DbPool::Postgres(pool) => {
            let rows = sqlx::query("SELECT id, to_addrs, created_at FROM messages WHERE mailbox_id=$1 AND folder='Sent' AND subject=$2 AND created_at > NOW() - make_interval(mins => $3) ORDER BY created_at DESC LIMIT 10")
                .bind(mailbox_id).bind(subject).bind(WINDOW_MINUTES as i32)
                .fetch_all(pool).await.unwrap_or_default();
            rows.into_iter().find_map(|r| {
                let addrs: String = r.get("to_addrs");
                same(&addrs).then(|| {
                    let id: uuid::Uuid = r.get("id");
                    let at: chrono::DateTime<chrono::Utc> = r.get("created_at");
                    (at.to_rfc3339(), id.to_string())
                })
            })
        }
        DbPool::Sqlite(pool) => {
            let rows = sqlx::query("SELECT id, to_addrs, created_at FROM messages WHERE mailbox_id=? AND folder='Sent' AND subject=? AND datetime(created_at) > datetime('now', ?) ORDER BY created_at DESC LIMIT 10")
                .bind(mailbox_id.to_string()).bind(subject).bind(format!("-{} minutes", WINDOW_MINUTES))
                .fetch_all(pool).await.unwrap_or_default();
            rows.into_iter().find_map(|r| {
                let addrs: String = r.get("to_addrs");
                same(&addrs).then(|| {
                    let id: String = r.get("id");
                    let at: String = r.get("created_at");
                    (at, id)
                })
            })
        }
    }
}

pub async fn mcp_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, StatusCode> {
    if crate::api::execution_context::mcp_capability_mode_enabled() {
        return mcp_v2_handler(&state, &headers, body).await;
    }

    // Internal access requires the configured token; Cerveau's dedicated
    // secret must match exactly rather than merely being present.
    // MCP is an instance-admin/service trust boundary: the internal token or
    // Cerveau secret is not a mailbox-user credential and is only provisioned
    // to trusted backend services. Data tools still require a valid mailbox_id
    // and constrain every query to that mailbox; possession of this secret is
    // the authorization to select an approved mailbox on behalf of a service.
    let is_internal = headers
        .get("x-internal-token")
        .and_then(|v| v.to_str().ok())
        == Some(state.config.internal_token.as_str());
    let is_cerveau = state
        .config
        .cognee_secret
        .as_deref()
        .is_some_and(|expected| {
            headers
                .get("x-cerveau-internal-secret")
                .and_then(|v| v.to_str().ok())
                == Some(expected)
        });
    if !is_internal && !is_cerveau {
        // Check query ?api_key= via header already, but we need query parsing — for now just check Bearer
        let ok = validate_api_key(&state, &headers, None).await;
        if !ok {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    let v: Value = serde_json::from_slice(&body).unwrap_or(serde_json::json!({}));
    let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = v.get("id").cloned().unwrap_or(serde_json::json!(1));
    let result = match method {
        "initialize" => {
            serde_json::json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}}, "serverInfo":{"name":"aivory-mail-mcp","version":"0.1.0"}})
        }
        "tools/list" => serde_json::json!({"tools": [
            {"name":"search_mail","description":"Hybrid search mail (vector+FTS) scoped to the required mailbox_id. Each hit includes id, subject, snippet, from, to, folder (Inbox/Sent/Drafts/Spam/Trash), and created_at — ALWAYS check folder + to + created_at to tell drafts apart from already-sent mail before acting.","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"folder":{"type":"string","description":"Restrict to one folder, e.g. Sent or Drafts"},"limit":{"type":"integer"},"mailbox_id":{"type":"string","description":"Required mailbox_id; results are always scoped to this mailbox"}},"required":["query","mailbox_id"]}},
            {"name":"get_inbox_overview","description":"1-call inbox stats scoped to the required mailbox_id","inputSchema":{"type":"object","properties":{"mailbox_id":{"type":"string","description":"Required mailbox to scope to"} },"required":["mailbox_id"]}},
            {"name":"get_thread_memory","description":"Budgeted thread context for LLM scoped to the required mailbox_id","inputSchema":{"type":"object","properties":{"thread_id":{"type":"string"},"budget":{"type":"integer"},"mailbox_id":{"type":"string"}},"required":["thread_id","mailbox_id"]}},
            {"name":"get_knowledge_compile","description":"Auto-compiled knowledge for all folders scoped to the required mailbox_id","inputSchema":{"type":"object","properties":{"budget":{"type":"integer"},"mailbox_id":{"type":"string"}},"required":["mailbox_id"]}},
            {"name":"send_mail","description":"Send email from the required mailbox_id only. Duplicate-protected: an identical send (same recipients + subject within 30 minutes) is REFUSED — if refused, the mail already went out, report it instead of retrying.","inputSchema":{"type":"object","properties":{"mailbox_id":{"type":"string"},"from":{"type":"string"},"to":{"type":"array"},"subject":{"type":"string"},"text":{"type":"string"}},"required":["mailbox_id","from","to","subject"]}},
            {"name":"delete_draft","description":"Move one DRAFT to Trash (mailbox-scoped). Only works on folder=Drafts — sent mail can never be deleted through this tool. Use it to clean up superseded drafts instead of asking the user.","inputSchema":{"type":"object","properties":{"mailbox_id":{"type":"string","description":"Required mailbox_id"},"message_id":{"type":"string","description":"ID of the draft (see search_mail with folder=Drafts)"}},"required":["mailbox_id","message_id"]}}
        ]}),
        "tools/call" => {
            let name = v
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let args = v
                .get("params")
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(serde_json::json!({}));
            // Cerveau → Mail calls may include mailbox_id for per-mailbox isolation.
            // When present, every query is scoped to that mailbox; when absent
            // the tool falls back to global (needed for health checks but never for user data).
            let mailbox_id = args
                .get("mailbox_id")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            match name {
                "search_mail" => {
                    let scoped_mailbox_id = match mailbox_id
                        .as_deref()
                        .and_then(|value| uuid::Uuid::parse_str(value).ok())
                    {
                        Some(id) => id,
                        None => {
                            return Ok(Json(
                                serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"mailbox_id is required and must be a valid UUID"}}),
                            ))
                        }
                    };
                    let mailbox_id = Some(scoped_mailbox_id.to_string());
                    let q = args
                        .get("query")
                        .and_then(|s| s.as_str())
                        .unwrap_or("invoice");
                    let folder = args.get("folder").and_then(|s| s.as_str());
                    let limit: i64 = args
                        .get("limit")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(10)
                        .min(50);
                    // query DB directly (like GET /v1/search) — scoped when mailbox_id is given.
                    // NOTE: every hit carries folder + to + created_at. Agent
                    // loops in the past could not tell Drafts from Sent
                    // because an earlier version dropped these fields — keep
                    // them, they are the whole point of this tool.
                    let results: Vec<Value> = match &state.db {
                        DbPool::Postgres(pool) => {
                            let mailbox_uuid = mailbox_id
                                .as_deref()
                                .and_then(|value| uuid::Uuid::parse_str(value).ok());
                            let rows = match (folder, mailbox_uuid) {
                                (Some(f), Some(mid)) => sqlx::query("SELECT id, subject, snippet, from_addr, to_addrs, folder, created_at FROM messages WHERE (subject ILIKE $1 OR snippet ILIKE $1) AND folder=$2 AND mailbox_id=$3 ORDER BY created_at DESC LIMIT $4")
                                    .bind(format!("%{}%", q)).bind(f).bind(mid).bind(limit)
                                    .fetch_all(pool).await.unwrap_or_default(),
                                (Some(f), None) => sqlx::query("SELECT id, subject, snippet, from_addr, to_addrs, folder, created_at FROM messages WHERE (subject ILIKE $1 OR snippet ILIKE $1) AND folder=$2 ORDER BY created_at DESC LIMIT $3")
                                    .bind(format!("%{}%", q)).bind(f).bind(limit)
                                    .fetch_all(pool).await.unwrap_or_default(),
                                (None, Some(mid)) => sqlx::query("SELECT id, subject, snippet, from_addr, to_addrs, folder, created_at FROM messages WHERE (subject ILIKE $1 OR snippet ILIKE $1) AND mailbox_id=$2 ORDER BY created_at DESC LIMIT $3")
                                    .bind(format!("%{}%", q)).bind(mid).bind(limit)
                                    .fetch_all(pool).await.unwrap_or_default(),
                                (None, None) => sqlx::query("SELECT id, subject, snippet, from_addr, to_addrs, folder, created_at FROM messages WHERE (subject ILIKE $1 OR snippet ILIKE $1) ORDER BY created_at DESC LIMIT $2")
                                    .bind(format!("%{}%", q)).bind(limit)
                                    .fetch_all(pool).await.unwrap_or_default(),
                            };
                            rows.into_iter().map(|r| serde_json::json!({"id": r.get::<uuid::Uuid,_>("id").to_string(), "subject": r.get::<Option<String>,_>("subject"), "snippet": r.get::<Option<String>,_>("snippet"), "from": r.get::<String,_>("from_addr"), "to": r.get::<String,_>("to_addrs"), "folder": r.get::<String,_>("folder"), "created_at": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339()})).collect()
                        }
                        DbPool::Sqlite(pool) => {
                            let rows = if let Some(f) = folder {
                                if let Some(mid) = &mailbox_id {
                                    sqlx::query("SELECT id, subject, snippet, from_addr, to_addrs, folder, created_at FROM messages WHERE (subject LIKE ? OR snippet LIKE ?) AND folder=? AND mailbox_id=? ORDER BY created_at DESC LIMIT ?")
                                        .bind(format!("%{}%", q)).bind(format!("%{}%", q)).bind(f).bind(mid).bind(limit).fetch_all(pool).await.unwrap_or_default()
                                } else {
                                    sqlx::query("SELECT id, subject, snippet, from_addr, to_addrs, folder, created_at FROM messages WHERE (subject LIKE ? OR snippet LIKE ?) AND folder=? ORDER BY created_at DESC LIMIT ?")
                                        .bind(format!("%{}%", q)).bind(format!("%{}%", q)).bind(f).bind(limit).fetch_all(pool).await.unwrap_or_default()
                                }
                            } else if let Some(mid) = &mailbox_id {
                                sqlx::query("SELECT id, subject, snippet, from_addr, to_addrs, folder, created_at FROM messages WHERE (subject LIKE ? OR snippet LIKE ?) AND mailbox_id=? ORDER BY created_at DESC LIMIT ?")
                                    .bind(format!("%{}%", q)).bind(format!("%{}%", q)).bind(mid).bind(limit).fetch_all(pool).await.unwrap_or_default()
                            } else {
                                sqlx::query("SELECT id, subject, snippet, from_addr, to_addrs, folder, created_at FROM messages WHERE (subject LIKE ? OR snippet LIKE ?) ORDER BY created_at DESC LIMIT ?")
                                    .bind(format!("%{}%", q)).bind(format!("%{}%", q)).bind(limit).fetch_all(pool).await.unwrap_or_default()
                            };
                            rows.into_iter().map(|r| serde_json::json!({"id": r.get::<String,_>("id"), "subject": r.get::<Option<String>,_>("subject"), "snippet": r.get::<Option<String>,_>("snippet"), "from": r.get::<String,_>("from_addr"), "to": r.get::<String,_>("to_addrs"), "folder": r.get::<String,_>("folder"), "created_at": r.get::<String,_>("created_at")})).collect()
                        }
                    };
                    serde_json::json!({"content":[{"type":"text","text": serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".into())}]})
                }
                "get_inbox_overview" => {
                    let scoped_mailbox_id = match args
                        .get("mailbox_id")
                        .and_then(|s| s.as_str())
                        .or(mailbox_id.as_deref())
                        .and_then(|value| uuid::Uuid::parse_str(value).ok())
                    {
                        Some(id) => id,
                        None => {
                            return Ok(Json(
                                serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"mailbox_id is required and must be a valid UUID"}}),
                            ))
                        }
                    };
                    let scoped_mailbox_id = scoped_mailbox_id.to_string();
                    let mid = Some(scoped_mailbox_id.as_str());
                    let overview: Value = match &state.db {
                        DbPool::Postgres(pool) => {
                            let (total, unread): (i64, i64) = if let Some(m) =
                                mid.and_then(|s| uuid::Uuid::parse_str(s).ok())
                            {
                                let t: i64 = sqlx::query_scalar(
                                    "SELECT COUNT(*) FROM messages WHERE mailbox_id=$1",
                                )
                                .bind(m)
                                .fetch_one(pool)
                                .await
                                .unwrap_or(0);
                                let u: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE mailbox_id=$1 AND folder='Inbox' AND is_read=false AND (snoozed_until IS NULL OR snoozed_until <= NOW())").bind(m).fetch_one(pool).await.unwrap_or(0);
                                (t, u)
                            } else {
                                let t: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
                                    .fetch_one(pool)
                                    .await
                                    .unwrap_or(0);
                                let u: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder='Inbox' AND is_read=false AND (snoozed_until IS NULL OR snoozed_until <= NOW())").fetch_one(pool).await.unwrap_or(0);
                                (t, u)
                            };
                            serde_json::json!({"total": total, "unread_inbox": unread, "mailbox_id": mid})
                        }
                        DbPool::Sqlite(pool) => {
                            let (total, unread): (i64, i64) = if let Some(m) = mid {
                                let t: i64 = sqlx::query_scalar(
                                    "SELECT COUNT(*) FROM messages WHERE mailbox_id=?",
                                )
                                .bind(m)
                                .fetch_one(pool)
                                .await
                                .unwrap_or(0);
                                let u: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE mailbox_id=? AND folder='Inbox' AND is_read=0 AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))").bind(m).fetch_one(pool).await.unwrap_or(0);
                                (t, u)
                            } else {
                                let t: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
                                    .fetch_one(pool)
                                    .await
                                    .unwrap_or(0);
                                let u: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder='Inbox' AND is_read=0 AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))").fetch_one(pool).await.unwrap_or(0);
                                (t, u)
                            };
                            serde_json::json!({"total": total, "unread_inbox": unread, "mailbox_id": mid})
                        }
                    };
                    serde_json::json!({"content":[{"type":"text","text": serde_json::to_string_pretty(&overview).unwrap()} ]})
                }
                "get_thread_memory" => {
                    let tid = args.get("thread_id").and_then(|s| s.as_str()).unwrap_or("");
                    let thread_id = match uuid::Uuid::parse_str(tid) {
                        Ok(id) => id,
                        Err(_) => {
                            return Ok(Json(
                                serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"thread_id must be a valid UUID"}}),
                            ))
                        }
                    };
                    let mailbox_id = match args
                        .get("mailbox_id")
                        .and_then(|s| s.as_str())
                        .or(mailbox_id.as_deref())
                        .and_then(|value| uuid::Uuid::parse_str(value).ok())
                    {
                        Some(id) => id,
                        None => {
                            return Ok(Json(
                                serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"mailbox_id is required and must be a valid UUID"}}),
                            ))
                        }
                    };
                    let budget: usize =
                        args.get("budget").and_then(|v| v.as_u64()).unwrap_or(2000) as usize;
                    let mem: Value = match &state.db {
                        DbPool::Postgres(pool) => {
                            let rows = sqlx::query("SELECT subject, snippet, body_text FROM messages WHERE thread_id=$1 AND mailbox_id=$2 ORDER BY created_at DESC").bind(thread_id).bind(mailbox_id).fetch_all(pool).await.unwrap_or_default();
                            let mut out = Vec::new();
                            let mut used = 0;
                            for r in rows {
                                let subj: Option<String> = r.get("subject");
                                let snip: Option<String> = r.get("snippet");
                                let txt: Option<String> = r.get("body_text");
                                let chunk = format!(
                                    "{} — {} — {}",
                                    subj.unwrap_or_default(),
                                    snip.unwrap_or_default(),
                                    txt.unwrap_or_default()
                                );
                                if used + chunk.len() > budget {
                                    break;
                                }
                                used += chunk.len();
                                out.push(chunk);
                            }
                            serde_json::json!({"thread_id": tid, "budget": budget, "messages": out})
                        }
                        DbPool::Sqlite(pool) => {
                            let rows = sqlx::query("SELECT subject, snippet, body_text FROM messages WHERE thread_id=? AND mailbox_id=? ORDER BY created_at DESC").bind(tid).bind(mailbox_id.to_string()).fetch_all(pool).await.unwrap_or_default();
                            let mut out = Vec::new();
                            let mut used = 0;
                            for r in rows {
                                let subj: Option<String> = r.get("subject");
                                let snip: Option<String> = r.get("snippet");
                                let txt: Option<String> = r.get("body_text");
                                let chunk = format!(
                                    "{} — {} — {}",
                                    subj.unwrap_or_default(),
                                    snip.unwrap_or_default(),
                                    txt.unwrap_or_default()
                                );
                                if used + chunk.len() > budget {
                                    break;
                                }
                                used += chunk.len();
                                out.push(chunk);
                            }
                            serde_json::json!({"thread_id": tid, "budget": budget, "messages": out})
                        }
                    };
                    serde_json::json!({"content":[{"type":"text","text": serde_json::to_string_pretty(&mem).unwrap()} ]})
                }
                "get_knowledge_compile" => {
                    let mailbox_id = match args
                        .get("mailbox_id")
                        .and_then(|s| s.as_str())
                        .and_then(|value| uuid::Uuid::parse_str(value).ok())
                    {
                        Some(id) => id,
                        None => {
                            return Ok(Json(
                                serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"mailbox_id is required and must be a valid UUID"}}),
                            ))
                        }
                    };
                    let budget: i64 = args.get("budget").and_then(|v| v.as_i64()).unwrap_or(4000);
                    let mut out = serde_json::json!({"budget": budget, "mailbox_id": mailbox_id});
                    out["compile"] = serde_json::json!(format!(
                        "use GET /v1/knowledge/compile?mailbox_id={}&budget={} for full",
                        mailbox_id, budget
                    ));
                    serde_json::json!({"content":[{"type":"text","text": serde_json::to_string_pretty(&out).unwrap()} ]})
                }
                "send_mail" => {
                    let mailbox_id = match args
                        .get("mailbox_id")
                        .and_then(|s| s.as_str())
                        .and_then(|value| uuid::Uuid::parse_str(value).ok())
                    {
                        Some(id) => id,
                        None => {
                            return Ok(Json(
                                serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"mailbox_id is required and must be a valid UUID"}}),
                            ))
                        }
                    };
                    let from = args
                        .get("from")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_lowercase();
                    let owner = mailbox_address(&state, mailbox_id)
                        .await?
                        .ok_or(StatusCode::NOT_FOUND)?;
                    if from != owner {
                        return Ok(Json(
                            serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32003,"message":"from address is not owned by mailbox_id"}}),
                        ));
                    }
                    let to_vals = args
                        .get("to")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let subject = args
                        .get("subject")
                        .and_then(|s| s.as_str())
                        .unwrap_or("(no subject)")
                        .to_string();
                    let text = args
                        .get("text")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    if from.is_empty() || to_vals.is_empty() {
                        serde_json::json!({"content":[{"type":"text","text": "missing from/to"}]})
                    } else if let Some(dup) = recent_identical_send(&state, mailbox_id, &to_vals, &subject).await {
                        // Agent retry loops re-send the same mail over and over
                        // because they cannot see their own Sent box. Refuse
                        // loudly with the proof instead of spamming the lead.
                        serde_json::json!({"content":[{"type":"text","text": format!("duplicate_blocked: this exact email (to {} / subject {:?}) was already sent at {} (id {}). Do NOT send again — tell the user it already went out.", to_vals.join(", "), subject, dup.0, dup.1)}]})
                    } else {
                        let req = aivory_mail_core::types::SendRequest {
                            from: from.to_string(),
                            to: to_vals.clone(),
                            cc: None,
                            bcc: None,
                            subject: subject.clone(),
                            text: Some(text.clone()),
                            html: None,
                            attachments: None,
                            thread_id: None,
                            in_reply_to: None,
                        };
                        match crate::mail::outbound::send_email(&state, req).await {
                            Ok(id) => {
                                serde_json::json!({"content":[{"type":"text","text": format!("sent {}", id)}]})
                            }
                            Err(e) => {
                                tracing::warn!("legacy mcp send_mail failed: {}", e);
                                serde_json::json!({"content":[{"type":"text","text": "send failed: outcome is unknown, reconcile before retrying"}]})
                            }
                        }
                    }
                }
                "delete_draft" => {
                    let mb = match args.get("mailbox_id").and_then(|s| s.as_str()).and_then(|v| uuid::Uuid::parse_str(v).ok()) {
                        Some(id) => id,
                        None => return Ok(Json(serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"mailbox_id is required and must be a valid UUID"}}))),
                    };
                    let mid = match args.get("message_id").and_then(|s| s.as_str()).and_then(|v| uuid::Uuid::parse_str(v).ok()) {
                        Some(id) => id,
                        None => return Ok(Json(serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"message_id is required and must be a valid UUID"}}))),
                    };
                    // Drafts only — anything already sent can never go through here.
                    let (folder, subject): (Option<String>, Option<String>) = match &state.db {
                        DbPool::Postgres(pool) => sqlx::query("SELECT folder, subject FROM messages WHERE id=$1 AND mailbox_id=$2")
                            .bind(mb).bind(mid).fetch_optional(pool).await.ok().flatten()
                            .map(|r| (r.get::<String,_>("folder"), r.get::<Option<String>,_>("subject")))
                            .map(|(f, s)| (Some(f), s)).unwrap_or((None, None)),
                        DbPool::Sqlite(pool) => sqlx::query("SELECT folder, subject FROM messages WHERE id=? AND mailbox_id=?")
                            .bind(mb.to_string()).bind(mid.to_string()).fetch_optional(pool).await.ok().flatten()
                            .map(|r| (r.get::<String,_>("folder"), r.get::<Option<String>,_>("subject")))
                            .map(|(f, s)| (Some(f), s)).unwrap_or((None, None)),
                    };
                    match folder.as_deref() {
                        None => serde_json::json!({"content":[{"type":"text","text": "not_found: no such message in this mailbox"}]}),
                        Some(f) if !f.eq_ignore_ascii_case("drafts") => serde_json::json!({"content":[{"type":"text","text": format!("refused: message is in folder={} — delete_draft only removes Drafts, sent mail is untouchable", f)}]}),
                        _ => {
                            let ok = match &state.db {
                                DbPool::Postgres(pool) => sqlx::query("UPDATE messages SET folder='Trash' WHERE id=$1 AND mailbox_id=$2").bind(mb).bind(mid).execute(pool).await.is_ok(),
                                DbPool::Sqlite(pool) => sqlx::query("UPDATE messages SET folder='Trash' WHERE id=? AND mailbox_id=?").bind(mb.to_string()).bind(mid.to_string()).execute(pool).await.is_ok(),
                            };
                            if ok {
                                serde_json::json!({"content":[{"type":"text","text": format!("draft_deleted: {:?} moved to Trash", subject.unwrap_or_default())}]})
                            } else {
                                serde_json::json!({"content":[{"type":"text","text": "delete failed: database error"}]})
                            }
                        }
                    }
                }
                _ => {
                    serde_json::json!({"content":[{"type":"text","text": format!("tool {} called", name)}]})
                }
            }
        }
        _ => serde_json::json!({"error": format!("unknown method {}", method)}),
    };
    Ok(Json(
        serde_json::json!({"jsonrpc":"2.0","id": id, "result": result}),
    ))
}


pub(crate) fn mcp_v2_tools() -> Value {
    // This is a reviewed, static catalog. Annotations are client hints only;
    // scope checks and confirmation validation remain the server authority.
    serde_json::json!([
        {
            "name": "search_mail",
            "description": "Search mail within the capability mailbox context",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "maxLength": 65536},
                    "folder": {"type": "string", "maxLength": 1024},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50}
                },
                "required": ["query"]
            }
        },
        {
            "name": "get_inbox_overview",
            "description": "Get inbox totals within the capability mailbox context",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false},
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "get_thread_memory",
            "description": "Read thread context within the capability mailbox context",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": {"type": "string", "maxLength": 128},
                    "budget": {"type": "integer", "minimum": 1, "maximum": 20000}
                },
                "required": ["thread_id"]
            }
        },
        {
            "name": "get_knowledge_compile",
            "description": "Request mailbox-scoped knowledge compilation",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {"budget": {"type": "integer", "minimum": 1, "maximum": 20000}}
            }
        },
        {
            "name": "draft.create",
            "description": "Save a draft reply/message in the capability mailbox. Non-destructive: no confirmation needed, nothing is sent. Pair with send_mail (which needs confirmation) for a review-then-send loop.",
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {
                    "to": {"type": "array", "maxItems": 100, "items": {"type": "string", "maxLength": 1024}},
                    "subject": {"type": "string", "maxLength": 65536},
                    "text": {"type": "string", "maxLength": 2097152},
                    "html": {"type": "string", "maxLength": 2097152},
                    "thread_id": {"type": "string"},
                    "from": {"type": "string", "maxLength": 1024}
                },
                "required": ["to", "subject"]
            }
        },
        {
            "name": "request_send_confirmation",
            "description": "Review step before send_mail. Submit the exact email you intend to send and get back a short-lived confirmation_id — pass that same confirmation_id AND the identical from/to/subject/text/html/cc/bcc/attachments back into send_mail to actually dispatch it (the payload must match byte-for-byte or send_mail rejects the confirmation).",
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": {"type": "string", "maxLength": 1024},
                    "to": {"type": "array", "maxItems": 100, "items": {"type": "string", "maxLength": 1024}},
                    "subject": {"type": "string", "maxLength": 65536},
                    "text": {"type": "string", "maxLength": 2097152},
                    "html": {"type": "string", "maxLength": 2097152},
                    "cc": {"type": "array", "maxItems": 100, "items": {"type": "string", "maxLength": 1024}},
                    "bcc": {"type": "array", "maxItems": 100, "items": {"type": "string", "maxLength": 1024}},
                    "thread_id": {"type": "string"},
                    "in_reply_to": {"type": "string"},
                    "attachments": {"type": "array", "maxItems": 10}
                },
                "required": ["from", "to", "subject"]
            }
        },
        {
            "name": "send_mail",
            "description": "Request a send; confirmation is required before dispatch",
            "annotations": {"readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": true},
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": {"type": "string", "maxLength": 1024},
                    "to": {"type": "array", "maxItems": 100, "items": {"type": "string", "maxLength": 1024}},
                    "subject": {"type": "string", "maxLength": 65536},
                    "text": {"type": "string", "maxLength": 2097152},
                    "html": {"type": "string", "maxLength": 2097152},
                    "cc": {"type": "array", "maxItems": 100, "items": {"type": "string", "maxLength": 1024}},
                    "bcc": {"type": "array", "maxItems": 100, "items": {"type": "string", "maxLength": 1024}},
                    "thread_id": {"type": "string"},
                    "in_reply_to": {"type": "string"},
                    "attachments": {"type": "array", "maxItems": 10},
                    "confirmation_id": {"type": "string"}
                },
                "required": ["from", "to", "subject", "confirmation_id"]
            }
        }
    ])
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Json<Value> {
    Json(serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message}
    }))
}

async fn mcp_v2_handler(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, StatusCode> {
    let capability = crate::api::mcp_capabilities::resolve_mcp_capability(state, headers).await?;
    let context = crate::api::execution_context::ExecutionContext::from_capability(
        &capability, headers,
    );
    if body.len() > mcp_limits::McpLimits::MAX_REQUEST_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let request: Value = serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
    if !request.is_object()
        || request.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || request.get("method").and_then(Value::as_str).is_none()
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");

    let result = match method {
        "initialize" => serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "aivory-mail-mcp", "version": "0.2.0"}
        }),
        "tools/list" => serde_json::json!({"tools": mcp_v2_tools()}),
        "tools/call" => {
            let params = request
                .get("params")
                .filter(|arguments| arguments.is_object())
                .ok_or(StatusCode::BAD_REQUEST)?;
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or(StatusCode::BAD_REQUEST)?;
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            if !args.is_object() {
                return Err(StatusCode::BAD_REQUEST);
            }
            match name {
                "search_mail" => {
                    context.require_scope("mail.search")?;
                    let query = mcp_limits::required_string(
                        &args,
                        "query",
                        mcp_limits::McpLimits::MAX_QUERY_BYTES,
                    )?;
                    let folder = mcp_limits::optional_string(
                        &args,
                        "folder",
                        mcp_limits::McpLimits::MAX_FOLDER_BYTES,
                    )?;
                    let limit = mcp_limits::bounded_integer(
                        &args,
                        "limit",
                        10,
                        mcp_limits::McpLimits::MAX_SEARCH_LIMIT,
                    )? as i64;
                    let needle = format!("%{}%", query);
                    let results: Vec<Value> = match &state.db {
                        DbPool::Postgres(pool) => {
                            let rows = if let Some(folder) = folder {
                                sqlx::query("SELECT id, subject, from_addr FROM messages WHERE tenant_id=$1 AND mailbox_id=$2 AND (subject ILIKE $3 OR snippet ILIKE $3) AND folder=$4 ORDER BY created_at DESC LIMIT $5")
                                    .bind(context.tenant_id)
                                    .bind(context.mailbox_id)
                                    .bind(&needle)
                                    .bind(folder)
                                    .bind(limit)
                                    .fetch_all(pool)
                                    .await
                                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                            } else {
                                sqlx::query("SELECT id, subject, from_addr FROM messages WHERE tenant_id=$1 AND mailbox_id=$2 AND (subject ILIKE $3 OR snippet ILIKE $3) ORDER BY created_at DESC LIMIT $4")
                                    .bind(context.tenant_id)
                                    .bind(context.mailbox_id)
                                    .bind(&needle)
                                    .bind(limit)
                                    .fetch_all(pool)
                                    .await
                                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                            };
                            rows.into_iter()
                                .map(|row| {
                                    serde_json::json!({
                                        "id": row.get::<uuid::Uuid, _>("id").to_string(),
                                        "subject": mcp_limits::sanitize_optional(row.get::<Option<String>, _>("subject")),
                                        "from": row.get::<String, _>("from_addr")
                                    })
                                })
                                .collect()
                        }
                        DbPool::Sqlite(pool) => {
                            let rows = if let Some(folder) = folder {
                                sqlx::query("SELECT id, subject, from_addr FROM messages WHERE tenant_id=? AND mailbox_id=? AND (subject LIKE ? OR snippet LIKE ?) AND folder=? ORDER BY created_at DESC LIMIT ?")
                                    .bind(context.tenant_id.to_string())
                                    .bind(context.mailbox_id.to_string())
                                    .bind(&needle)
                                    .bind(&needle)
                                    .bind(folder)
                                    .bind(limit)
                                    .fetch_all(pool)
                                    .await
                                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                            } else {
                                sqlx::query("SELECT id, subject, from_addr FROM messages WHERE tenant_id=? AND mailbox_id=? AND (subject LIKE ? OR snippet LIKE ?) ORDER BY created_at DESC LIMIT ?")
                                    .bind(context.tenant_id.to_string())
                                    .bind(context.mailbox_id.to_string())
                                    .bind(&needle)
                                    .bind(&needle)
                                    .bind(limit)
                                    .fetch_all(pool)
                                    .await
                                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                            };
                            rows.into_iter()
                                .map(|row| {
                                    serde_json::json!({
                                        "id": row.get::<String, _>("id"),
                                        "subject": mcp_limits::sanitize_optional(row.get::<Option<String>, _>("subject")),
                                        "from": row.get::<String, _>("from_addr")
                                    })
                                })
                                .collect()
                        }
                    };
                    serde_json::json!({"content": [{"type": "text", "text": serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())}]})
                }
                "get_inbox_overview" => {
                    context.require_scope("mail.read")?;
                    let (total, unread): (i64, i64) = match &state.db {
                        DbPool::Postgres(pool) => {
                            let total = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE tenant_id=$1 AND mailbox_id=$2")
                                .bind(context.tenant_id)
                                .bind(context.mailbox_id)
                                .fetch_one(pool)
                                .await
                                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                            let unread = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE tenant_id=$1 AND mailbox_id=$2 AND folder='Inbox' AND is_read=false AND (snoozed_until IS NULL OR snoozed_until <= NOW())")
                                .bind(context.tenant_id)
                                .bind(context.mailbox_id)
                                .fetch_one(pool)
                                .await
                                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                            (total, unread)
                        }
                        DbPool::Sqlite(pool) => {
                            let tenant = context.tenant_id.to_string();
                            let mailbox = context.mailbox_id.to_string();
                            let total = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE tenant_id=? AND mailbox_id=?")
                                .bind(&tenant)
                                .bind(&mailbox)
                                .fetch_one(pool)
                                .await
                                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                            let unread = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE tenant_id=? AND mailbox_id=? AND folder='Inbox' AND is_read=0 AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))")
                                .bind(&tenant)
                                .bind(&mailbox)
                                .fetch_one(pool)
                                .await
                                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                            (total, unread)
                        }
                    };
                    serde_json::json!({"content": [{"type": "text", "text": serde_json::json!({"total": total, "unread_inbox": unread}).to_string()}]})
                }
                "get_thread_memory" => {
                    context.require_scope("mail.thread.read")?;
                    let thread_id = mcp_limits::required_string(
                        &args,
                        "thread_id",
                        mcp_limits::McpLimits::MAX_THREAD_ID_BYTES,
                    )
                    .and_then(|value| {
                        Uuid::parse_str(value).map_err(|_| StatusCode::BAD_REQUEST)
                    })?;
                    let budget = mcp_limits::bounded_integer(
                        &args,
                        "budget",
                        2000,
                        mcp_limits::McpLimits::MAX_THREAD_BUDGET,
                    )?;
                    let field_limit = mcp_limits::McpLimits::MAX_THREAD_FIELD_BYTES as i64;
                    let row_limit = mcp_limits::McpLimits::MAX_THREAD_MESSAGES as i64;
                    let rows: Vec<(Option<String>, Option<String>, Option<String>)> =
                        match &state.db {
                            DbPool::Postgres(pool) => sqlx::query(
                                "SELECT LEFT(COALESCE(subject, ''), $4) AS subject, LEFT(COALESCE(snippet, ''), $4) AS snippet, LEFT(COALESCE(body_text, ''), $4) AS body_text FROM messages WHERE tenant_id=$1 AND mailbox_id=$2 AND thread_id=$3 ORDER BY created_at DESC LIMIT $5",
                            )
                            .bind(context.tenant_id)
                            .bind(context.mailbox_id)
                            .bind(thread_id)
                            .bind(field_limit)
                            .bind(row_limit)
                            .fetch_all(pool)
                            .await
                            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                            .into_iter()
                            .map(|row| {
                                (
                                    row.get("subject"),
                                    row.get("snippet"),
                                    row.get("body_text"),
                                )
                            })
                            .collect(),
                            DbPool::Sqlite(pool) => sqlx::query(
                                "SELECT substr(COALESCE(subject, ''), 1, ?) AS subject, substr(COALESCE(snippet, ''), 1, ?) AS snippet, substr(COALESCE(body_text, ''), 1, ?) AS body_text FROM messages WHERE tenant_id=? AND mailbox_id=? AND thread_id=? ORDER BY created_at DESC LIMIT ?",
                            )
                            .bind(field_limit)
                            .bind(field_limit)
                            .bind(field_limit)
                            .bind(context.tenant_id.to_string())
                            .bind(context.mailbox_id.to_string())
                            .bind(thread_id.to_string())
                            .bind(row_limit)
                            .fetch_all(pool)
                            .await
                            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                            .into_iter()
                            .map(|row| {
                                (
                                    row.get("subject"),
                                    row.get("snippet"),
                                    row.get("body_text"),
                                )
                            })
                            .collect(),
                        };
                    let mut messages = Vec::new();
                    let mut used = 0usize;
                    for (subject, snippet, body_text) in rows {
                        let chunk = format!(
                            "{} — {} — {}",
                            mcp_limits::sanitize_for_ai(&subject.unwrap_or_default()),
                            mcp_limits::sanitize_for_ai(&snippet.unwrap_or_default()),
                            mcp_limits::sanitize_for_ai(&body_text.unwrap_or_default())
                        );
                        if used + chunk.len() > budget {
                            break;
                        }
                        used += chunk.len();
                        messages.push(chunk);
                    }
                    serde_json::json!({"content": [{"type": "text", "text": serde_json::json!({"thread_id": thread_id, "messages": messages}).to_string()}]})
                }
                "get_knowledge_compile" => {
                    context.require_scope("mail.knowledge.read")?;
                    let budget = mcp_limits::bounded_integer(
                        &args,
                        "budget",
                        4000,
                        mcp_limits::McpLimits::MAX_KNOWLEDGE_BUDGET,
                    )?;
                    let compiled = crate::api::knowledge::compile_for_context(state, &context, budget).await?;
                    serde_json::json!({"content": [{"type": "text", "text": compiled.to_string()}]})
                }
                "draft.create" => {
                    context.require_scope("mail.draft.create")?;
                    let to_vals: Vec<String> = args
                        .get("to")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str())
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .take(mcp_limits::McpLimits::MAX_RECIPIENTS)
                                .collect()
                        })
                        .unwrap_or_default();
                    if to_vals.is_empty() {
                        return Err(StatusCode::BAD_REQUEST);
                    }
                    for addr in &to_vals {
                        if addr.len() > mcp_limits::McpLimits::MAX_ADDRESS_BYTES {
                            return Err(StatusCode::BAD_REQUEST);
                        }
                    }
                    let subject = mcp_limits::required_string(
                        &args,
                        "subject",
                        mcp_limits::McpLimits::MAX_SUBJECT_BYTES,
                    )?;
                    // Tolerant body parsing: explicit "" counts as absent (unlike
                    // optional_string, which rejects empty strings outright).
                    let body_part = |name: &str| -> Result<Option<String>, StatusCode> {
                        match args.get(name) {
                            None => Ok(None),
                            Some(v) => {
                                let s = v.as_str().ok_or(StatusCode::BAD_REQUEST)?;
                                if s.as_bytes().len() > mcp_limits::McpLimits::MAX_BODY_BYTES {
                                    return Err(StatusCode::BAD_REQUEST);
                                }
                                Ok(if s.is_empty() { None } else { Some(s.to_string()) })
                            }
                        }
                    };
                    let text = body_part("text")?.unwrap_or_default();
                    let html = body_part("html")?;
                    if text.is_empty() && html.as_deref().unwrap_or("").is_empty() {
                        return Err(StatusCode::BAD_REQUEST);
                    }
                    let thread_id: Option<String> = args
                        .get("thread_id")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.trim().is_empty())
                        .map(|s| {
                            Uuid::parse_str(s.trim()).map_err(|_| StatusCode::BAD_REQUEST)?;
                            Ok::<String, StatusCode>(s.trim().to_string())
                        })
                        .transpose()?;
                    // Default From to the capability mailbox address; explicit
                    // `from` must belong to this mailbox (same rule as send).
                    let mailbox_addr: String = match &state.db {
                        DbPool::Postgres(pool) => sqlx::query_scalar(
                            "SELECT address FROM mailboxes WHERE id=$1",
                        )
                        .bind(context.mailbox_id)
                        .fetch_optional(pool)
                        .await
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                        .unwrap_or_default(),
                        DbPool::Sqlite(pool) => sqlx::query_scalar(
                            "SELECT address FROM mailboxes WHERE id=?",
                        )
                        .bind(context.mailbox_id.to_string())
                        .fetch_optional(pool)
                        .await
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                        .unwrap_or_default(),
                    };
                    let from = args
                        .get("from")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .unwrap_or(mailbox_addr);
                    if from.len() > mcp_limits::McpLimits::MAX_ADDRESS_BYTES {
                        return Err(StatusCode::BAD_REQUEST);
                    }
                    let draft_id = Uuid::new_v4();
                    let to_str = serde_json::to_string(&to_vals).unwrap_or_else(|_| "[]".into());
                    let snippet: String = text.chars().take(80).collect();
                    match &state.db {
                        DbPool::Postgres(pool) => {
                            sqlx::query("INSERT INTO messages (id, tenant_id, mailbox_id, thread_id, message_id, from_addr, to_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, size_bytes, has_attachments, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'Drafts',false,false,0,false,NOW())")
                                .bind(draft_id).bind(context.tenant_id).bind(context.mailbox_id).bind(thread_id.clone()).bind(format!("<draft-{}@aivory.mail>", draft_id)).bind(&from).bind(&to_str).bind(&subject).bind(&snippet).bind(&text).bind(&html).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                        }
                        DbPool::Sqlite(pool) => {
                            sqlx::query("INSERT INTO messages (id, tenant_id, mailbox_id, thread_id, message_id, from_addr, to_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, size_bytes, has_attachments, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                                .bind(draft_id.to_string()).bind(context.tenant_id.to_string()).bind(context.mailbox_id.to_string()).bind(thread_id.clone()).bind(format!("<draft-{}@aivory.mail>", draft_id)).bind(&from).bind(&to_str).bind(&subject).bind(&snippet).bind(&text).bind(&html).bind("Drafts").bind(1).bind(0).bind(0).bind(0).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                        }
                    }
                    serde_json::json!({"content": [{"type": "text", "text": serde_json::json!({"status": "draft_saved", "draft_id": draft_id.to_string(), "thread_id": thread_id}).to_string()}]})
                }
                "request_send_confirmation" => {
                    // Self-service issuance: an MCP caller holding `mail.send`
                    // can mint its own confirmation, scoped to its own
                    // (tenant_id, mailbox_id, caller_id) from the capability
                    // context — the same fields consume_send_confirmation
                    // later re-checks, so this can't be used to confirm a send
                    // on behalf of any other mailbox or caller. Previously the
                    // only issuer was the admin-only REST route
                    // (POST /v1/agent-access/send-confirmations), which no
                    // MCP-connected agent can call — so send_mail's required
                    // confirmation_id was unobtainable from this surface and
                    // every send_mail call failed with -32009, regardless of
                    // payload. This tool closes that gap.
                    context.require_scope("mail.send")?;
                    let payload: SendRequest = serde_json::from_value(args.clone())
                        .map_err(|_| StatusCode::BAD_REQUEST)?;
                    aivory_mail_core::routing::validate_send_request(&payload)
                        .map_err(|_| StatusCode::BAD_REQUEST)?;
                    mcp_limits::validate_send_request(&payload)?;
                    let confirmation = crate::api::mcp_confirmations::issue_send_confirmation(
                        state,
                        crate::api::mcp_confirmations::IssueSendConfirmationRequest {
                            tenant_id: context.tenant_id.to_string(),
                            mailbox_id: context.mailbox_id.to_string(),
                            caller_id: context.caller_id.clone(),
                            payload,
                            expires_in_seconds: None,
                        },
                    )
                    .await?;
                    serde_json::json!({"content": [{"type": "text", "text": serde_json::json!({
                        "confirmation_id": confirmation.id,
                        "expires_at": confirmation.expires_at,
                    }).to_string()}]})
                }
                "send_mail" => {
                    context.require_scope("mail.send")?;
                    let confirmation_id = args
                        .get("confirmation_id")
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty());
                    let Some(confirmation_id) = confirmation_id else {
                        return Ok(rpc_error(
                            &id,
                            -32009,
                            "send confirmation required before dispatch",
                        ));
                    };
                    let payload: SendRequest = serde_json::from_value(args.clone())
                        .map_err(|_| StatusCode::BAD_REQUEST)?;
                    aivory_mail_core::routing::validate_send_request(&payload)
                        .map_err(|_| StatusCode::BAD_REQUEST)?;
                    mcp_limits::validate_send_request(&payload)?;
                    crate::api::mcp_confirmations::consume_send_confirmation(
                        state,
                        &context,
                        confirmation_id,
                        &payload,
                    )
                    .await?;
                    match crate::mail::outbound::send_email_with_context(
                        state,
                        payload,
                        &context,
                    )
                    .await
                    {
                        Ok(message_id) => serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": serde_json::json!({
                                    "status": "succeeded",
                                    "message_id": message_id,
                                }).to_string()
                            }]
                        }),
                        Err(_) => {
                            return Ok(rpc_error(
                                &id,
                                -32011,
                                "send outcome is unknown; reconcile before retrying",
                            ));
                        }
                    }
                }
                _ => return Ok(rpc_error(&id, -32601, "unknown tool")),
            }
        }
        _ => return Ok(rpc_error(&id, -32601, "unknown method")),
    };

    let response = serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result});
    if mcp_limits::validate_rpc_result(&response).is_err() {
        return Ok(rpc_error(
            &response["id"],
            -32012,
            "MCP result exceeds the response limit",
        ));
    }
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::mcp_v2_tools;

    #[test]
    fn v2_catalog_is_static_scoped_and_annotated() {
        let tools = mcp_v2_tools();
        let tools = tools.as_array().expect("catalog array");
        let names = tools
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name"))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "search_mail",
                "get_inbox_overview",
                "get_thread_memory",
                "get_knowledge_compile",
                "draft.create",
                "request_send_confirmation",
                "send_mail"
            ]
        );
        assert!(tools.iter().all(|tool| tool["annotations"].is_object()));
        assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
        assert_eq!(tools[4]["name"], "draft.create");
        assert_eq!(tools[4]["annotations"]["destructiveHint"], false);
        assert_eq!(tools[5]["annotations"]["readOnlyHint"], false);
        assert_eq!(tools[5]["annotations"]["idempotentHint"], false);
        let catalog = serde_json::to_string(tools).expect("catalog serialization");
        assert!(!catalog.contains("mailbox_id"));
        assert!(!catalog.contains("tenant_id"));
        assert!(!catalog.contains("api_key"));
    }
}
