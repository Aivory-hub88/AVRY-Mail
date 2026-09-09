use crate::api::AppState;
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

pub async fn mcp_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, StatusCode> {
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
            {"name":"search_mail","description":"Hybrid search mail (vector+FTS) scoped to the required mailbox_id","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"folder":{"type":"string"},"limit":{"type":"integer"},"mailbox_id":{"type":"string","description":"Required mailbox_id; results are always scoped to this mailbox"}},"required":["query","mailbox_id"]}},
            {"name":"get_inbox_overview","description":"1-call inbox stats scoped to the required mailbox_id","inputSchema":{"type":"object","properties":{"mailbox_id":{"type":"string","description":"Required mailbox to scope to"} },"required":["mailbox_id"]}},
            {"name":"get_thread_memory","description":"Budgeted thread context for LLM scoped to the required mailbox_id","inputSchema":{"type":"object","properties":{"thread_id":{"type":"string"},"budget":{"type":"integer"},"mailbox_id":{"type":"string"}},"required":["thread_id","mailbox_id"]}},
            {"name":"get_knowledge_compile","description":"Auto-compiled knowledge for all folders scoped to the required mailbox_id","inputSchema":{"type":"object","properties":{"budget":{"type":"integer"},"mailbox_id":{"type":"string"}},"required":["mailbox_id"]}},
            {"name":"send_mail","description":"Send email from the required mailbox_id only","inputSchema":{"type":"object","properties":{"mailbox_id":{"type":"string"},"from":{"type":"string"},"to":{"type":"array"},"subject":{"type":"string"},"text":{"type":"string"}},"required":["mailbox_id","from","to","subject"]}}
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
                    // query DB directly (like GET /v1/search) — scoped when mailbox_id is given
                    let results: Vec<Value> = match &state.db {
                        DbPool::Postgres(pool) => {
                            let mut sql = String::from("SELECT id, subject, snippet, from_addr, folder FROM messages WHERE (subject ILIKE $1 OR snippet ILIKE $1)");
                            if let Some(f) = folder {
                                sql.push_str(&format!(" AND folder='{}'", f.replace('\'', "''")));
                            }
                            if let Some(mid) = &mailbox_id {
                                if let Ok(uid) = uuid::Uuid::parse_str(mid) {
                                    sql.push_str(&format!(" AND mailbox_id='{}'", uid));
                                }
                            }
                            sql.push_str(" ORDER BY created_at DESC LIMIT $2");
                            let rows = sqlx::query(&sql)
                                .bind(format!("%{}%", q))
                                .bind(limit)
                                .fetch_all(pool)
                                .await
                                .unwrap_or_default();
                            rows.into_iter().map(|r| serde_json::json!({"id": r.get::<uuid::Uuid,_>("id").to_string(), "subject": r.get::<Option<String>,_>("subject"), "from": r.get::<String,_>("from_addr")})).collect()
                        }
                        DbPool::Sqlite(pool) => {
                            let rows = if let Some(f) = folder {
                                if let Some(mid) = &mailbox_id {
                                    sqlx::query("SELECT id, subject, snippet, from_addr FROM messages WHERE (subject LIKE ? OR snippet LIKE ?) AND folder=? AND mailbox_id=? ORDER BY created_at DESC LIMIT ?")
                                        .bind(format!("%{}%", q)).bind(format!("%{}%", q)).bind(f).bind(mid).bind(limit).fetch_all(pool).await.unwrap_or_default()
                                } else {
                                    sqlx::query("SELECT id, subject, snippet, from_addr FROM messages WHERE (subject LIKE ? OR snippet LIKE ?) AND folder=? ORDER BY created_at DESC LIMIT ?")
                                        .bind(format!("%{}%", q)).bind(format!("%{}%", q)).bind(f).bind(limit).fetch_all(pool).await.unwrap_or_default()
                                }
                            } else if let Some(mid) = &mailbox_id {
                                sqlx::query("SELECT id, subject, snippet, from_addr FROM messages WHERE (subject LIKE ? OR snippet LIKE ?) AND mailbox_id=? ORDER BY created_at DESC LIMIT ?")
                                    .bind(format!("%{}%", q)).bind(format!("%{}%", q)).bind(mid).bind(limit).fetch_all(pool).await.unwrap_or_default()
                            } else {
                                sqlx::query("SELECT id, subject, snippet, from_addr FROM messages WHERE (subject LIKE ? OR snippet LIKE ?) ORDER BY created_at DESC LIMIT ?")
                                    .bind(format!("%{}%", q)).bind(format!("%{}%", q)).bind(limit).fetch_all(pool).await.unwrap_or_default()
                            };
                            rows.into_iter().map(|r| serde_json::json!({"id": r.get::<String,_>("id"), "subject": r.get::<Option<String>,_>("subject"), "from": r.get::<String,_>("from_addr")})).collect()
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
                                serde_json::json!({"content":[{"type":"text","text": format!("send failed: {}", e)}]})
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
