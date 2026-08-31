use aivory_mail_core::{parser::{parse_raw_email, snippet_from_body}, intelligence};
use aivory_mail_storage::object_store::ObjectStore;
use anyhow::{bail, Result};
use chrono::Utc;
use sqlx::Row;
use tracing::{info, warn};
use uuid::Uuid;

use crate::api::AppState;
use std::sync::Arc;

pub async fn handle_inbound_raw(
    state: &Arc<AppState>,
    from: &str,
    to: &str,
    raw: Vec<u8>,
) -> Result<Uuid> {
    let parsed = parse_raw_email(&raw)?;
    info!("inbound parsed from={} to={} subject={:?} size={}", from, to, parsed.subject, raw.len());

    // 1. Resolve mailbox — reject mail for addresses nobody owns instead of
    // silently storing it under an orphaned tenant (matches real MTA behavior;
    // the SMTP ingress also checks this at RCPT TO, this is the webhook-path backstop).
    let resolution = crate::mail::routing::resolve_recipient(state, to).await?;
    if !resolution.accept {
        bail!("recipient rejected: {} ({})", to, resolution.reason);
    }
    let mailbox_id = resolution.mailbox_id.unwrap_or_else(Uuid::nil);
    let tenant_id = resolution.tenant_id.unwrap_or_else(Uuid::nil);

    // 2. Store raw to R2/S3/local
    let raw_key = format!("raw/{}/{}.eml", Utc::now().format("%Y/%m/%d"), Uuid::new_v4());
    state.store.put(&raw_key, raw.clone(), "message/rfc822").await?;

    // 3. Intelligence (heuristic + optional AI gateway)
    let subject = parsed.subject.clone().unwrap_or_default();
    let body_for_ai = parsed.body_text.clone().or(parsed.body_html.clone()).unwrap_or_default();
    let intel = intelligence::analyze(&subject, &body_for_ai);

    // 4. Insert message into DB — routed through enabled filters first
    let msg_id = Uuid::new_v4();
    let thread_id = find_or_create_thread(state, &mailbox_id, &subject, from).await?;
    let snippet = snippet_from_body(parsed.body_text.as_deref(), parsed.body_html.as_deref(), 160);
    let headers_json = serde_json::to_value(&parsed.headers).unwrap_or(serde_json::Value::Null);
    let msg_uid = parsed.message_id.clone().unwrap_or_else(|| format!("<{}@aivory.local>", msg_id));
    let folder = resolve_filtered_folder(state, from, &subject, &body_for_ai).await;

    insert_message(state, &msg_id, &tenant_id, &mailbox_id, &thread_id, &msg_uid, &parsed, &snippet, &raw_key, &headers_json, &folder).await?;

    // 5. Store attachments
    for att in &parsed.attachments {
        let att_id = Uuid::new_v4();
        let filename = att.filename.clone().unwrap_or_else(|| "attachment.bin".into());
        let key = format!("attachments/{}/{}/{}", msg_id, att_id, filename);
        state.store.put(&key, att.data.clone(), &att.content_type).await?;
        insert_attachment(state, &att_id, &msg_id, &filename, &att.content_type, att.data.len() as i32, &key).await?;
    }

    // 5b. Vacation auto-reply (fire-and-forget — reuses the Phase 1 outbound
    // path, so it's DKIM-signed and gated on the mailbox's domain being verified)
    {
        let state_vac = state.clone();
        let from_vac = from.to_string();
        let subject_vac = subject.clone();
        let mailbox_vac = mailbox_id;
        tokio::spawn(async move {
            if let Err(e) = maybe_send_vacation_reply(&state_vac, &mailbox_vac, &from_vac, &subject_vac).await {
                tracing::warn!("vacation auto-reply check failed: {}", e);
            }
        });
    }

    // 6. Trigger workflow / AI gateway async (fire-and-forget)
    // 6b. Cognee graph_remember (sidecar t_<user>.mail_ops) — shape B bulk ingest, non-blocking
    {
        let state_cog = state.clone();
        let subj_cog = subject.clone();
        let body_cog = body_for_ai.clone();
        let mid_cog = msg_id.to_string();
        let tenant_cog = tenant_id.to_string();
        tokio::spawn(async move {
            let agent_type = std::env::var("COGNEE_AGENT_TYPE").unwrap_or_else(|_| "mail_ops".into());
            if let Err(e) = crate::mail::cognee_client::remember_email(&tenant_cog, &agent_type, &subj_cog, &body_cog, &mid_cog).await {
                tracing::warn!("cognee remember failed: {}", e);
            }
        });
        let _ = state_cog; // keep for workflow below if needed
    }
    let state_clone = state.clone();
    let intel_clone = intel.clone();
    let subject_clone = subject.clone();
    tokio::spawn(async move {
        if let Err(e) = trigger_intelligence_hooks(&state_clone, &msg_id, &subject_clone, &body_for_ai, &intel_clone).await {
            warn!("intelligence hook failed: {}", e);
        }
    });

    // 7. Realtime push
    state.hub.broadcast_new_message(&mailbox_id.to_string(), &serde_json::json!({
        "id": msg_id.to_string(),
        "from": parsed.from_addr,
        "subject": parsed.subject,
        "snippet": snippet,
        "intelligence": intel,
    })).await;

    // 8. Workflow trigger (n8n / Aivory Workflow)
    if let Some(wf_url) = &state.config.workflow_url {
        let wf_url = wf_url.clone();
        let payload = serde_json::json!({
            "event": "email.received",
            "message_id": msg_id.to_string(),
            "from": from, "to": to,
            "subject": subject,
            "snippet": snippet,
            "intelligence": intel,
        });
        tokio::spawn(async move {
            let _ = reqwest::Client::new().post(format!("{}/webhook/email-received", wf_url))
                .json(&payload).send().await;
        });
    }

    Ok(msg_id)
}

async fn find_or_create_thread(state: &Arc<AppState>, mailbox_id: &Uuid, subject: &str, _from: &str) -> Result<Option<Uuid>> {
    // normalize subject: strip Re:/Fwd:
    let norm_subject = subject.trim().trim_start_matches(|c: char| c == ' ').to_string();
    let norm = regex::Regex::new(r"(?i)^(re|fwd|fw):\s*").unwrap().replace(&norm_subject, "").to_string();
    // try find recent thread with same normalized subject
    let thread_id = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT id FROM threads WHERE mailbox_id = $1 AND lower(subject) = lower($2) ORDER BY last_message_at DESC LIMIT 1")
                .bind(mailbox_id).bind(&norm).fetch_optional(pool).await?;
            row.map(|r| r.get::<Uuid,_>("id"))
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id FROM threads WHERE mailbox_id = ? AND lower(subject) = lower(?) ORDER BY last_message_at DESC LIMIT 1")
                .bind(mailbox_id.to_string()).bind(&norm).fetch_optional(pool).await?;
            row.map(|r| { let s: String = r.get("id"); Uuid::parse_str(&s).unwrap_or(Uuid::nil()) })
        }
    };
    if let Some(id) = thread_id { return Ok(Some(id)); }
    // create new thread
    let new_id = Uuid::new_v4();
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO threads (id, tenant_id, mailbox_id, subject, participant_addrs, message_count, last_message_at, has_unread) VALUES ($1,$2,$3,$4,$5,1,NOW(),true)")
                .bind(new_id).bind(Uuid::nil()).bind(mailbox_id).bind(&norm)
                .bind(serde_json::json!([_from]).to_string())
                .execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO threads (id, tenant_id, mailbox_id, subject, participant_addrs, message_count, last_message_at, has_unread) VALUES (?,?,?,?,?,1,?,1)")
                .bind(new_id.to_string()).bind(Uuid::nil().to_string()).bind(mailbox_id.to_string()).bind(&norm)
                .bind(serde_json::json!([_from]).to_string()).bind(Utc::now().to_rfc3339())
                .execute(pool).await?;
        }
    }
    Ok(Some(new_id))
}

async fn insert_message(
    state: &Arc<AppState>, id: &Uuid, tenant_id: &Uuid, mailbox_id: &Uuid, thread_id: &Option<Uuid>,
    msg_uid: &str, parsed: &aivory_mail_core::parser::ParsedEmail, snippet: &str, raw_key: &str, headers_json: &serde_json::Value,
    folder: &str,
) -> Result<()> {
    let from_addr = parsed.from_addr.clone().unwrap_or_default();
    let from_name = parsed.from_name.clone();
    let to_json = serde_json::to_string(&parsed.to_addrs).unwrap();
    let cc_json = serde_json::to_string(&parsed.cc_addrs).unwrap();
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query(r#"INSERT INTO messages (id, tenant_id, mailbox_id, thread_id, message_id, from_addr, from_name, to_addrs, cc_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, raw_r2_key, size_bytes, has_attachments, headers_json, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,false,false,$15,$16,$17,$18,NOW())"#)
                .bind(id).bind(tenant_id).bind(mailbox_id).bind(thread_id).bind(msg_uid)
                .bind(&from_addr).bind(&from_name).bind(&to_json).bind(&cc_json)
                .bind(&parsed.subject).bind(snippet).bind(&parsed.body_text).bind(&parsed.body_html)
                .bind(folder).bind(raw_key).bind(parsed.raw_size as i32).bind(!parsed.attachments.is_empty()).bind(headers_json)
                .execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query(r#"INSERT INTO messages (id, tenant_id, mailbox_id, thread_id, message_id, from_addr, from_name, to_addrs, cc_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, raw_r2_key, size_bytes, has_attachments, headers_json, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"#)
                .bind(id.to_string()).bind(tenant_id.to_string()).bind(mailbox_id.to_string()).bind(thread_id.map(|u| u.to_string()))
                .bind(msg_uid).bind(&from_addr).bind(&from_name).bind(&to_json).bind(&cc_json)
                .bind(&parsed.subject).bind(snippet).bind(&parsed.body_text).bind(&parsed.body_html)
                .bind(folder).bind(0).bind(0).bind(raw_key).bind(parsed.raw_size as i32).bind(if parsed.attachments.is_empty(){0}else{1}).bind(headers_json.to_string())
                .bind(Utc::now().to_rfc3339())
                .execute(pool).await?;
        }
    }
    Ok(())
}

/// Loads enabled `mail_filters` and runs them (aivory_mail_core::filters)
/// against this message. Falls back to "Inbox" on no match or on error —
/// a broken filter must never block mail delivery.
async fn resolve_filtered_folder(state: &Arc<AppState>, from: &str, subject: &str, body: &str) -> String {
    let rows: Vec<(String, String)> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("SELECT criteria_json, action_json FROM mail_filters WHERE enabled=true ORDER BY created_at ASC")
                .fetch_all(pool).await.ok()
                .map(|rs| rs.into_iter().map(|r| (r.get("criteria_json"), r.get("action_json"))).collect())
                .unwrap_or_default()
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("SELECT criteria_json, action_json FROM mail_filters WHERE enabled=1 ORDER BY created_at ASC")
                .fetch_all(pool).await.ok()
                .map(|rs| rs.into_iter().map(|r| (r.get("criteria_json"), r.get("action_json"))).collect())
                .unwrap_or_default()
        }
    };
    if rows.is_empty() { return "Inbox".to_string(); }
    let parsed: Vec<(serde_json::Value, serde_json::Value)> = rows.iter()
        .map(|(c, a)| (serde_json::from_str(c).unwrap_or_default(), serde_json::from_str(a).unwrap_or_default()))
        .collect();
    let rules: Vec<aivory_mail_core::filters::FilterRule> = parsed.iter()
        .map(|(c, a)| aivory_mail_core::filters::FilterRule { criteria: c, action: a })
        .collect();
    aivory_mail_core::filters::resolve_folder(&rules, from, subject, body).unwrap_or_else(|| "Inbox".to_string())
}

const AUTO_REPLY_SKIP_PREFIXES: &[&str] = &["no-reply@", "noreply@", "mailer-daemon@", "postmaster@"];

/// Sends at most one vacation auto-reply per sender per `interval_days`,
/// only while `enabled` and (if set) within the start/end window.
async fn maybe_send_vacation_reply(state: &Arc<AppState>, mailbox_id: &Uuid, from: &str, subject: &str) -> Result<()> {
    let sender = from.trim().to_lowercase();
    if sender.is_empty() || AUTO_REPLY_SKIP_PREFIXES.iter().any(|p| sender.starts_with(p)) {
        return Ok(());
    }

    #[derive(sqlx::FromRow)]
    struct VacationRow { enabled: bool, subject: String, body: String, interval_days: i32 }

    let vac: Option<VacationRow> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query_as("SELECT enabled, subject, body, interval_days FROM vacation_responders WHERE mailbox_id=$1 AND (start_at IS NULL OR start_at <= NOW()) AND (end_at IS NULL OR end_at >= NOW())")
                .bind(mailbox_id).fetch_optional(pool).await?
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT enabled, subject, body, interval_days FROM vacation_responders WHERE mailbox_id=? AND (start_at IS NULL OR start_at <= datetime('now')) AND (end_at IS NULL OR end_at >= datetime('now'))")
                .bind(mailbox_id.to_string()).fetch_optional(pool).await?;
            row.map(|r| VacationRow { enabled: r.get::<i32,_>("enabled") != 0, subject: r.get("subject"), body: r.get("body"), interval_days: r.get("interval_days") })
        }
    };
    let Some(vac) = vac else { return Ok(()) };
    if !vac.enabled { return Ok(()); }

    let mailbox_id_str = mailbox_id.to_string();
    let already_sent = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("SELECT 1 FROM vacation_replies_sent WHERE mailbox_id=$1 AND sender_addr=$2 AND sent_at > NOW() - ($3 || ' days')::interval")
                .bind(&mailbox_id_str).bind(&sender).bind(vac.interval_days.max(1).to_string())
                .fetch_optional(pool).await?.is_some()
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT sent_at FROM vacation_replies_sent WHERE mailbox_id=? AND sender_addr=?")
                .bind(&mailbox_id_str).bind(&sender).fetch_optional(pool).await?;
            match row {
                Some(r) => {
                    let sent_at: String = r.get("sent_at");
                    chrono::DateTime::parse_from_rfc3339(&sent_at).ok()
                        .map(|dt| (Utc::now() - dt.with_timezone(&Utc)).num_days() < vac.interval_days.max(1) as i64)
                        .unwrap_or(false)
                }
                None => false,
            }
        }
    };
    if already_sent { return Ok(()); }

    let mailbox_addr: Option<String> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query_scalar("SELECT address FROM mailboxes WHERE id=$1").bind(mailbox_id).fetch_optional(pool).await?
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query_scalar("SELECT address FROM mailboxes WHERE id=?").bind(&mailbox_id_str).fetch_optional(pool).await?
        }
    };
    let Some(mailbox_addr) = mailbox_addr else { return Ok(()) };

    let reply_subject = if vac.subject.trim().is_empty() { format!("Re: {}", subject) } else { vac.subject.clone() };

    let req = aivory_mail_core::types::SendRequest {
        from: mailbox_addr, to: vec![sender.clone()], cc: None, bcc: None,
        subject: reply_subject, text: Some(vac.body.clone()), html: None,
        attachments: None, thread_id: None, in_reply_to: None,
    };
    crate::mail::outbound::send_email(state, req).await?;

    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO vacation_replies_sent (mailbox_id, sender_addr, sent_at) VALUES ($1,$2,NOW()) ON CONFLICT (mailbox_id, sender_addr) DO UPDATE SET sent_at=NOW()")
                .bind(&mailbox_id_str).bind(&sender).execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("INSERT OR REPLACE INTO vacation_replies_sent (mailbox_id, sender_addr, sent_at) VALUES (?,?,?)")
                .bind(&mailbox_id_str).bind(&sender).bind(Utc::now().to_rfc3339()).execute(pool).await?;
        }
    }
    Ok(())
}

async fn insert_attachment(state: &Arc<AppState>, id: &Uuid, msg_id: &Uuid, filename: &str, ct: &str, size: i32, key: &str) -> Result<()> {
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO attachments (id, message_id, filename, content_type, size_bytes, r2_key) VALUES ($1,$2,$3,$4,$5,$6)")
                .bind(id).bind(msg_id).bind(filename).bind(ct).bind(size).bind(key).execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO attachments (id, message_id, filename, content_type, size_bytes, r2_key) VALUES (?,?,?,?,?,?)")
                .bind(id.to_string()).bind(msg_id.to_string()).bind(filename).bind(ct).bind(size).bind(key).execute(pool).await?;
        }
    }
    Ok(())
}

async fn trigger_intelligence_hooks(state: &Arc<AppState>, msg_id: &Uuid, subject: &str, body: &str, intel: &aivory_mail_core::types::IntelligenceResult) -> Result<()> {
    if let Some(ai_url) = &state.config.ai_gateway_url {
        let payload = serde_json::json!({
            "event": "mail.intelligence",
            "message_id": msg_id.to_string(),
            "subject": subject,
            "body_preview": &body[..body.len().min(2000)],
            "heuristic": intel,
            "model": state.config.mail_intelligence_model,
        });
        let _ = reqwest::Client::new().post(format!("{}/v1/mail/intelligence", ai_url))
            .header("x-internal-token", &state.config.internal_token)
            .json(&payload).timeout(std::time::Duration::from_secs(5)).send().await;
    }
    Ok(())
}
