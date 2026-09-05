use aivory_mail_core::{parser::{parse_raw_email, snippet_from_body}, intelligence, types::SendRequest};
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

    // 3b. Contacts upsert + block check (Mailflare parity)
    let from_email = parsed.from_addr.clone().unwrap_or_default().to_lowercase();
    let from_name = parsed.from_name.clone().unwrap_or_default();
    crate::api::contacts::upsert_from_address(&state.db, &from_email, &from_name).await;
    let mut folder = if is_blocked(&state.db, &from_email).await { "Spam".to_string() } else { "Inbox".to_string() };
    // 3c. Routing rules (filters) — priority + reject/block/forward (Mailflare parity)
    match apply_filters(&state.db, &from_email, &subject, &body_for_ai).await {
        Some(aivory_mail_core::filters::FilterAction::Reject(reason)) => {
            bail!("550 5.7.1 rejected by filter: {}", reason);
        }
        Some(aivory_mail_core::filters::FilterAction::Block) => {
            // auto-block sender and route to Spam
            let _ = crate::api::contacts::upsert_from_address(&state.db, &from_email, &from_name).await;
            // use block helper to also create filter, but avoid recursion
            folder = "Spam".to_string();
        }
        Some(aivory_mail_core::filters::FilterAction::Forward(addr)) => {
            // forward copy now, keep original in Inbox (Mailflare: store + forward)
            let fwd_req = SendRequest { from: to.to_string(), to: vec![addr.clone()], cc: None, bcc: None, subject: subject.clone(), text: Some(body_for_ai.clone()), html: None, attachments: None, thread_id: None, in_reply_to: None };
            let state_fw = state.clone();
            tokio::spawn(async move { let _ = crate::mail::outbound::send_email(&state_fw, fwd_req).await; });
            // keep folder as Inbox unless filter also had move
            if folder == "Spam" {} else { folder = "Inbox".to_string(); }
        }
        Some(aivory_mail_core::filters::FilterAction::Move(f)) => { folder = f; }
        _ => {
            // fallback to core filters resolver (origin/main logic) if no direct match
            let resolved = resolve_filtered_folder(state, &from_email, &subject, &body_for_ai).await;
            if resolved != "Inbox" { folder = resolved; }
        }
    }

    // 4. Insert message into DB
    let msg_id = Uuid::new_v4();
    let thread_id = find_or_create_thread(state, &mailbox_id, &subject, from).await?;
    let snippet = snippet_from_body(parsed.body_text.as_deref(), parsed.body_html.as_deref(), 160);
    let headers_json = serde_json::to_value(&parsed.headers).unwrap_or(serde_json::Value::Null);
    let msg_uid = parsed.message_id.clone().unwrap_or_else(|| format!("<{}@aivory.local>", msg_id));

    insert_message(state, &msg_id, &tenant_id, &mailbox_id, &thread_id, &msg_uid, &parsed, &snippet, &raw_key, &headers_json, &folder).await?;

    // 4b. Vacation auto-reply (async, dedup by interval_days)
    {
        let state_v = state.clone();
        let mailbox_v = mailbox_id;
        let from_v = parsed.from_addr.clone().unwrap_or_else(|| from.to_string());
        let headers_v = parsed.headers.clone();
        tokio::spawn(async move {
            let _ = maybe_send_vacation_reply(&state_v, &mailbox_v, &from_v, &headers_v).await;
        });
    }

    // 4c. Forwarding (if enabled for mailbox, forward copy)
    {
        let state_f = state.clone();
        let mailbox_f = mailbox_id;
        let subject_f = subject.clone();
        let body_f = body_for_ai.clone();
        let from_f = from.to_string();
        let to_f = to.to_string();
        tokio::spawn(async move {
            let _ = maybe_forward(&state_f, &mailbox_f, &from_f, &to_f, &subject_f, &body_f).await;
        });
    }

    // 5. Store attachments
    for att in &parsed.attachments {
        let att_id = Uuid::new_v4();
        let filename = att.filename.clone().unwrap_or_else(|| "attachment.bin".into());
        let key = format!("attachments/{}/{}/{}", msg_id, att_id, filename);
        state.store.put(&key, att.data.clone(), &att.content_type).await?;
        insert_attachment(state, &att_id, &msg_id, &filename, &att.content_type, att.data.len() as i32, &key).await?;
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

    // 7b. Webhooks dispatch (Mailflare parity) — async fire to all enabled webhooks for email.received
    {
        let state_wh = state.clone();
        let payload_wh = serde_json::json!({
            "event": "email.received",
            "message_id": msg_id.to_string(),
            "mailbox_id": mailbox_id.to_string(),
            "from": parsed.from_addr,
            "to": to,
            "subject": subject,
            "snippet": snippet,
            "folder": folder,
            "intelligence": intel,
        });
        tokio::spawn(async move { crate::api::webhooks_registry::trigger_for_event(&state_wh, "email.received", payload_wh).await; });
    }

    // 7c. Agent tasks auto-create for high signal (Mailflare agent inbox parity)
    {
        let state_ag = state.clone();
        let intel_ag = intel.clone();
        let subject_ag = subject.clone();
        let msg_ag = msg_id;
        let mb_ag = mailbox_id;
        let thr_ag = thread_id;
        tokio::spawn(async move {
            let state_str = if intel_ag.urgency == aivory_mail_core::types::Urgency::High { "needs_reply" } else if intel_ag.intent == "invoice" { "needs_approval" } else { "fyi" };
            // only create for actionable intents to avoid noise
            if intel_ag.intent == "invoice" || intel_ag.intent == "meeting_request" || intel_ag.urgency == aivory_mail_core::types::Urgency::High {
                let _ = crate::api::agent_tasks::create(
                    axum::extract::State(state_ag.clone()),
                    axum::Json(serde_json::json!({
                        "type": intel_ag.intent,
                        "state": state_str,
                        "title": format!("[{}] {}", intel_ag.intent, subject_ag),
                        "body": format!("Auto triage: {} urgency {}", intel_ag.intent, format!("{:?}", intel_ag.urgency)),
                        "mailbox_id": mb_ag.to_string(),
                        "thread_id": thr_ag.map(|u| u.to_string()),
                        "message_id": msg_ag.to_string(),
                        "payload": {"intelligence": intel_ag}
                    }))
                ).await;
            }
        });
    }

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

async fn is_blocked(db: &aivory_mail_storage::db::DbPool, email: &str) -> bool {
    if email.is_empty() { return false; }
    match db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            let r: Option<bool> = sqlx::query_scalar("SELECT blocked FROM contacts WHERE tenant_id='default' AND lower(email)=lower($1) LIMIT 1").bind(email).fetch_optional(pool).await.unwrap_or(None).flatten();
            r.unwrap_or(false)
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let r: Option<i32> = sqlx::query_scalar("SELECT blocked FROM contacts WHERE tenant_id='default' AND lower(email)=lower(?) LIMIT 1").bind(email).fetch_optional(pool).await.unwrap_or(None).flatten();
            r.map(|v| v != 0).unwrap_or(false)
        }
    }
}

async fn apply_filters(db: &aivory_mail_storage::db::DbPool, from: &str, subject: &str, body: &str) -> Option<aivory_mail_core::filters::FilterAction> {
    use aivory_mail_core::filters::{FilterAction, FilterRule};
    let rows: Vec<(String,String,i32)> = match db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            let rows = sqlx::query("SELECT criteria_json, action_json, COALESCE(priority,0) as priority FROM mail_filters WHERE tenant_id='default' AND enabled=true ORDER BY priority ASC, created_at ASC").fetch_all(pool).await.ok()?;
            rows.into_iter().map(|r| (r.get::<String,_>("criteria_json"), r.get::<String,_>("action_json"), r.get::<i32,_>("priority"))).collect()
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let rows = sqlx::query("SELECT criteria_json, action_json, COALESCE(priority,0) as priority FROM mail_filters WHERE tenant_id='default' AND enabled=1 ORDER BY priority ASC, created_at ASC").fetch_all(pool).await.ok()?;
            rows.into_iter().map(|r| (r.get::<String,_>("criteria_json"), r.get::<String,_>("action_json"), r.get::<i32,_>("priority"))).collect()
        }
    };
    let parsed: Vec<(serde_json::Value, serde_json::Value, i32)> = rows.iter().map(|(c,a,p)| (serde_json::from_str(c).unwrap_or_default(), serde_json::from_str(a).unwrap_or_default(), *p)).collect();
    let rules: Vec<FilterRule> = parsed.iter().map(|(c,a,p)| FilterRule { criteria: c, action: a, priority: *p }).collect();
    match aivory_mail_core::filters::resolve_action(&rules, from, subject, body) {
        FilterAction::None => None,
        other => Some(other),
    }
}

async fn maybe_send_vacation_reply(state: &Arc<AppState>, mailbox_id: &Uuid, from_email: &str, headers: &Vec<(String, String)>) -> Result<()> {
    // skip auto-replies and self
    let lower_headers: std::collections::HashMap<String,String> = headers.iter().map(|(k,v)| (k.to_lowercase(), v.to_lowercase())).collect();
    if lower_headers.get("auto-submitted").map(|v| v != "no").unwrap_or(false) { return Ok(()); }
    if lower_headers.get("x-auto-response-suppress").is_some() { return Ok(()); }
    if lower_headers.get("precedence").map(|v| v=="bulk"||v=="junk").unwrap_or(false) { return Ok(()); }
    // get vacation responder
    let (enabled, subject, body, interval_days, start_at, end_at): (bool, String, String, i32, Option<String>, Option<String>) = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            if let Some(row) = sqlx::query("SELECT enabled, subject, body, interval_days, start_at, end_at FROM vacation_responders WHERE mailbox_id=$1 LIMIT 1").bind(mailbox_id).fetch_optional(pool).await? {
                (row.get::<bool,_>("enabled"), row.get::<String,_>("subject"), row.get::<String,_>("body"), row.get::<i32,_>("interval_days"), row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("start_at").map(|d| d.to_rfc3339()), row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("end_at").map(|d| d.to_rfc3339()))
            } else { return Ok(()); }
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            if let Some(row) = sqlx::query("SELECT enabled, subject, body, interval_days, start_at, end_at FROM vacation_responders WHERE mailbox_id=? LIMIT 1").bind(mailbox_id.to_string()).fetch_optional(pool).await? {
                (row.get::<i32,_>("enabled")!=0, row.get::<String,_>("subject"), row.get::<String,_>("body"), row.get::<i32,_>("interval_days"), row.get::<Option<String>,_>("start_at"), row.get::<Option<String>,_>("end_at"))
            } else { return Ok(()); }
        }
    };
    if !enabled || body.is_empty() { return Ok(()); }
    let now = Utc::now();
    if let Some(s) = start_at { if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) { if now < dt.with_timezone(&Utc) { return Ok(()); } } }
    if let Some(e) = end_at { if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&e) { if now > dt.with_timezone(&Utc) { return Ok(()); } } }
    // dedup by interval
    let last_sent: Option<chrono::DateTime<Utc>> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query_scalar::<_, chrono::DateTime<Utc>>("SELECT sent_at FROM vacation_deliveries WHERE mailbox_id=$1 AND recipient=lower($2) LIMIT 1").bind(mailbox_id).bind(from_email.to_lowercase()).fetch_optional(pool).await?
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let s: Option<String> = sqlx::query_scalar("SELECT sent_at FROM vacation_deliveries WHERE mailbox_id=? AND lower(recipient)=lower(?) LIMIT 1").bind(mailbox_id.to_string()).bind(from_email.to_lowercase()).fetch_optional(pool).await?;
            s.and_then(|v| chrono::DateTime::parse_from_rfc3339(&v).ok().map(|d| d.with_timezone(&Utc)))
        }
    };
    if let Some(ls) = last_sent { if (now - ls).num_days() < interval_days as i64 { return Ok(()); } }
    // get mailbox address for From
    let mailbox_addr: String = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query_scalar("SELECT address FROM mailboxes WHERE id=$1").bind(mailbox_id).fetch_optional(pool).await?.unwrap_or_default()
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query_scalar("SELECT address FROM mailboxes WHERE id=?").bind(mailbox_id.to_string()).fetch_optional(pool).await?.unwrap_or_default()
        }
    };
    if mailbox_addr.is_empty() || mailbox_addr.to_lowercase() == from_email.to_lowercase() { return Ok(()); }
    let vac_subject = if subject.is_empty() { "Out of office".to_string() } else { subject };
    let vac_body = body;
    // send via outbound (will handle cloudflare vs smtp)
    let req = SendRequest { from: mailbox_addr.clone(), to: vec![from_email.to_string()], cc: None, bcc: None, subject: vac_subject, text: Some(vac_body.clone()), html: None, attachments: None, thread_id: None, in_reply_to: None };
    let _ = crate::mail::outbound::send_email(state, req).await;
    // record delivery
    let now_str = now.to_rfc3339();
    let did = Uuid::new_v4();
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            let _ = sqlx::query("INSERT INTO vacation_deliveries (id, mailbox_id, recipient, sent_at) VALUES ($1,$2,lower($3),$4) ON CONFLICT (mailbox_id, recipient) DO UPDATE SET sent_at=$4").bind(did).bind(mailbox_id).bind(from_email.to_lowercase()).bind(now).execute(pool).await;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let _ = sqlx::query("INSERT INTO vacation_deliveries (id, mailbox_id, recipient, sent_at) VALUES (?,?,?,?) ON CONFLICT(mailbox_id, recipient) DO UPDATE SET sent_at=excluded.sent_at").bind(did.to_string()).bind(mailbox_id.to_string()).bind(from_email.to_lowercase()).bind(&now_str).execute(pool).await;
        }
    }
    Ok(())
}

async fn maybe_forward(state: &Arc<AppState>, mailbox_id: &Uuid, from: &str, to: &str, subject: &str, body: &str) -> Result<()> {
    // Check forwarding settings for this mailbox (or tenant default)
    let forward_to: Option<String> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            // try mailbox-specific first
            let r: Option<String> = sqlx::query_scalar("SELECT value FROM user_settings WHERE category='forwarding' AND key='forward_to' AND mailbox_id=$1 LIMIT 1").bind(mailbox_id).fetch_optional(pool).await.unwrap_or(None).flatten();
            if let Some(v) = r { if !v.trim().is_empty() { Some(v) } else { None } } else {
                let r2: Option<String> = sqlx::query_scalar("SELECT value FROM user_settings WHERE category='forwarding' AND key='forward_to' AND (mailbox_id IS NULL OR mailbox_id='') LIMIT 1").fetch_optional(pool).await.unwrap_or(None).flatten();
                r2.filter(|v| !v.trim().is_empty())
            }
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let r: Option<String> = sqlx::query_scalar("SELECT value FROM user_settings WHERE category='forwarding' AND key='forward_to' AND mailbox_id=? LIMIT 1").bind(mailbox_id.to_string()).fetch_optional(pool).await.unwrap_or(None).flatten();
            if let Some(v) = r { if !v.trim().is_empty() { Some(v) } else { None } } else {
                let r2: Option<String> = sqlx::query_scalar("SELECT value FROM user_settings WHERE category='forwarding' AND key='forward_to' AND (mailbox_id IS NULL OR mailbox_id='' OR mailbox_id='default') LIMIT 1").fetch_optional(pool).await.unwrap_or(None).flatten();
                r2.filter(|v| !v.trim().is_empty())
            }
        }
    };
    let Some(fwd) = forward_to else { return Ok(()); };
    let fwd = fwd.trim().to_string();
    if fwd.is_empty() || fwd.eq_ignore_ascii_case(to) || fwd.eq_ignore_ascii_case(from) { return Ok(()); }
    // get keep_copy setting
    let keep_copy: bool = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            let v: Option<String> = sqlx::query_scalar("SELECT value FROM user_settings WHERE category='forwarding' AND key='keep_copy' AND (mailbox_id=$1 OR mailbox_id IS NULL) ORDER BY mailbox_id DESC NULLS LAST LIMIT 1").bind(mailbox_id).fetch_optional(pool).await.unwrap_or(None).flatten();
            v.map(|s| s=="true").unwrap_or(true)
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let v: Option<String> = sqlx::query_scalar("SELECT value FROM user_settings WHERE category='forwarding' AND key='keep_copy' AND (mailbox_id=? OR mailbox_id='' OR mailbox_id='default') LIMIT 1").bind(mailbox_id.to_string()).fetch_optional(pool).await.unwrap_or(None).flatten();
            v.map(|s| s=="true").unwrap_or(true)
        }
    };
    // get mailbox address for From (use original To as From? Use mailbox address)
    let mailbox_addr: String = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => sqlx::query_scalar("SELECT address FROM mailboxes WHERE id=$1").bind(mailbox_id).fetch_optional(pool).await.unwrap_or(None).unwrap_or_default(),
        aivory_mail_storage::db::DbPool::Sqlite(pool) => sqlx::query_scalar("SELECT address FROM mailboxes WHERE id=?").bind(mailbox_id.to_string()).fetch_optional(pool).await.unwrap_or(None).unwrap_or_default(),
    };
    if mailbox_addr.is_empty() { return Ok(()); }
    let fwd_subject = format!("Fwd: {}", subject);
    let fwd_body = format!("Forwarded message from {} to {}:\n\nSubject: {}\n\n{}", from, to, subject, body);
    let req = SendRequest { from: mailbox_addr, to: vec![fwd], cc: None, bcc: None, subject: fwd_subject, text: Some(fwd_body), html: None, attachments: None, thread_id: None, in_reply_to: None };
    let _ = crate::mail::outbound::send_email(state, req).await;
    // if not keep_copy, the original is kept but could be moved to Archive to simulate "forward and delete"
    if !keep_copy {
        // move original to Archive to indicate forwarded and not kept in Inbox
        // we don't have msg_id here, so we rely on the caller to handle via folder logic? For now just log
        tracing::info!("forwarded without keep_copy for mailbox {}", mailbox_id);
    }
    Ok(())
}

async fn insert_message(
    state: &Arc<AppState>, id: &Uuid, tenant_id: &Uuid, mailbox_id: &Uuid, thread_id: &Option<Uuid>,
    msg_uid: &str, parsed: &aivory_mail_core::parser::ParsedEmail, snippet: &str, raw_key: &str, headers_json: &serde_json::Value, folder: &str,
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
    let rows: Vec<(String, String, i32)> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("SELECT criteria_json, action_json, COALESCE(priority,0) FROM mail_filters WHERE enabled=true ORDER BY priority ASC, created_at ASC")
                .fetch_all(pool).await.ok()
                .map(|rs| rs.into_iter().map(|r| (r.get("criteria_json"), r.get("action_json"), r.get::<i32,_>("priority"))).collect())
                .unwrap_or_default()
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("SELECT criteria_json, action_json, COALESCE(priority,0) FROM mail_filters WHERE enabled=1 ORDER BY priority ASC, created_at ASC")
                .fetch_all(pool).await.ok()
                .map(|rs| rs.into_iter().map(|r| (r.get("criteria_json"), r.get("action_json"), r.get::<i32,_>("priority"))).collect())
                .unwrap_or_default()
        }
    };
    if rows.is_empty() { return "Inbox".to_string(); }
    let parsed: Vec<(serde_json::Value, serde_json::Value, i32)> = rows.iter()
        .map(|(c, a, p)| (serde_json::from_str(c).unwrap_or_default(), serde_json::from_str(a).unwrap_or_default(), *p))
        .collect();
    let rules: Vec<aivory_mail_core::filters::FilterRule> = parsed.iter()
        .map(|(c, a, p)| aivory_mail_core::filters::FilterRule { criteria: c, action: a, priority: *p })
        .collect();
    aivory_mail_core::filters::resolve_folder(&rules, from, subject, body).unwrap_or_else(|| "Inbox".to_string())
}

const AUTO_REPLY_SKIP_PREFIXES: &[&str] = &["no-reply@", "noreply@", "mailer-daemon@", "postmaster@"];

/// Legacy vacation reply via vacation_replies_sent (origin/main) — kept for compatibility with older data
#[allow(dead_code)]
async fn maybe_send_vacation_reply_simple(state: &Arc<AppState>, mailbox_id: &Uuid, from: &str, subject: &str) -> Result<()> {
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

/// Bulk historical import (used by the `import_mail` bin to bring in .eml
/// backlogs from a previous provider). Unlike `handle_inbound_raw` this:
///   - stores directly into the given `mailbox_id`/`folder` instead of
///     resolving+classifying the recipient (the folder is already known from
///     the export's directory structure — Inbox/Sent/Drafts/etc — and for
///     Sent/Drafts the envelope recipient is a third party, not the mailbox
///     owner, so routing resolution doesn't apply);
///   - skips vacation auto-reply, forwarding, webhooks, agent-task creation
///     and AI/Cognee hooks entirely — replaying those against a years-old
///     backlog would auto-reply to old senders and spam every integration.
/// Returns `Ok(None)` (no-op) if a message with the same Message-ID already
/// exists in this mailbox, so the import is safe to re-run.
pub async fn import_message(
    state: &Arc<AppState>,
    tenant_id: &Uuid,
    mailbox_id: &Uuid,
    folder: &str,
    raw: Vec<u8>,
) -> Result<Option<Uuid>> {
    let parsed = parse_raw_email(&raw)?;
    let msg_uid = parsed.message_id.clone().unwrap_or_else(|| format!("<import-{}@aivory.local>", Uuid::new_v4()));

    let already_exists = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM messages WHERE mailbox_id=$1 AND message_id=$2")
                .bind(mailbox_id).bind(&msg_uid).fetch_one(pool).await.unwrap_or(0) > 0
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM messages WHERE mailbox_id=? AND message_id=?")
                .bind(mailbox_id.to_string()).bind(&msg_uid).fetch_one(pool).await.unwrap_or(0) > 0
        }
    };
    if already_exists {
        return Ok(None);
    }

    let raw_key = format!("raw/import/{}/{}.eml", Utc::now().format("%Y/%m/%d"), Uuid::new_v4());
    state.store.put(&raw_key, raw.clone(), "message/rfc822").await?;

    let msg_id = Uuid::new_v4();
    let from = parsed.from_addr.clone().unwrap_or_default();
    let subject = parsed.subject.clone().unwrap_or_default();
    let thread_id = find_or_create_thread(state, mailbox_id, &subject, &from).await?;
    let snippet = snippet_from_body(parsed.body_text.as_deref(), parsed.body_html.as_deref(), 160);
    let headers_json = serde_json::to_value(&parsed.headers).unwrap_or(serde_json::Value::Null);

    insert_message(state, &msg_id, tenant_id, mailbox_id, &thread_id, &msg_uid, &parsed, &snippet, &raw_key, &headers_json, folder).await?;

    for att in &parsed.attachments {
        let att_id = Uuid::new_v4();
        let filename = att.filename.clone().unwrap_or_else(|| "attachment.bin".into());
        let key = format!("attachments/{}/{}/{}", msg_id, att_id, filename);
        state.store.put(&key, att.data.clone(), &att.content_type).await?;
        insert_attachment(state, &att_id, &msg_id, &filename, &att.content_type, att.data.len() as i32, &key).await?;
    }

    Ok(Some(msg_id))
}
