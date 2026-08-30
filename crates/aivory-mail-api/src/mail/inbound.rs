use aivory_mail_core::{parser::{parse_raw_email, snippet_from_body}, intelligence};
use aivory_mail_storage::object_store::ObjectStore;
use anyhow::Result;
use chrono::Utc;
use sqlx::{Row, SqlitePool, PgPool};
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

    // 1. Store raw to R2/S3/local
    let raw_key = format!("raw/{}/{}.eml", Utc::now().format("%Y/%m/%d"), Uuid::new_v4());
    state.store.put(&raw_key, raw.clone(), "message/rfc822").await?;

    // 2. Resolve mailbox
    let mailbox = resolve_mailbox(state, to).await?;
    let mailbox_id = mailbox.map(|m| m.0).unwrap_or_else(|| Uuid::new_v4()); // fallback if no mailbox (store anyway)
    let tenant_id = mailbox.map(|m| m.1).unwrap_or_else(|| Uuid::nil());

    // 3. Intelligence (heuristic + optional AI gateway)
    let subject = parsed.subject.clone().unwrap_or_default();
    let body_for_ai = parsed.body_text.clone().or(parsed.body_html.clone()).unwrap_or_default();
    let intel = intelligence::analyze(&subject, &body_for_ai);

    // 4. Insert message into DB
    let msg_id = Uuid::new_v4();
    let thread_id = find_or_create_thread(state, &mailbox_id, &subject, from).await?;
    let snippet = snippet_from_body(parsed.body_text.as_deref(), parsed.body_html.as_deref(), 160);
    let headers_json = serde_json::to_value(&parsed.headers).unwrap_or(serde_json::Value::Null);
    let msg_uid = parsed.message_id.clone().unwrap_or_else(|| format!("<{}@aivory.local>", msg_id));

    insert_message(state, &msg_id, &tenant_id, &mailbox_id, &thread_id, &msg_uid, &parsed, &snippet, &raw_key, &headers_json).await?;

    // 5. Store attachments
    for att in &parsed.attachments {
        let att_id = Uuid::new_v4();
        let filename = att.filename.clone().unwrap_or_else(|| "attachment.bin".into());
        let key = format!("attachments/{}/{}/{}", msg_id, att_id, filename);
        state.store.put(&key, att.data.clone(), &att.content_type).await?;
        insert_attachment(state, &att_id, &msg_id, &filename, &att.content_type, att.data.len() as i32, &key).await?;
    }

    // 6. Trigger workflow / AI gateway async (fire-and-forget)
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

async fn resolve_mailbox(state: &Arc<AppState>, to: &str) -> Result<Option<(Uuid, Uuid)>> {
    let norm = to.trim().to_lowercase();
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT id, tenant_id FROM mailboxes WHERE lower(address) = $1 LIMIT 1")
                .bind(&norm).fetch_optional(pool).await?;
            Ok(row.map(|r| (r.get::<Uuid,_>("id"), r.get::<Uuid,_>("tenant_id"))))
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id, tenant_id FROM mailboxes WHERE lower(address) = ? LIMIT 1")
                .bind(&norm).fetch_optional(pool).await?;
            Ok(row.map(|r| {
                let id_s: String = r.get("id");
                let tid_s: String = r.get("tenant_id");
                (Uuid::parse_str(&id_s).unwrap_or(Uuid::nil()), Uuid::parse_str(&tid_s).unwrap_or(Uuid::nil()))
            }))
        }
    }
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
            sqlx::query("INSERT INTO threads (id, tenant_id, mailbox_id, subject, participant_addrs, message_count, last_message_at, has_unread) VALUES (?,?,?,?,?,?,?,1)")
                .bind(new_id.to_string()).bind(Uuid::nil().to_string()).bind(mailbox_id.to_string()).bind(&norm)
                .bind(serde_json::json!([_from]).to_string()).bind(Utc::now().to_rfc3339()).bind(1)
                .execute(pool).await?;
        }
    }
    Ok(Some(new_id))
}

async fn insert_message(
    state: &Arc<AppState>, id: &Uuid, tenant_id: &Uuid, mailbox_id: &Uuid, thread_id: &Option<Uuid>,
    msg_uid: &str, parsed: &aivory_mail_core::parser::ParsedEmail, snippet: &str, raw_key: &str, headers_json: &serde_json::Value,
) -> Result<()> {
    let from_addr = parsed.from_addr.clone().unwrap_or_default();
    let from_name = parsed.from_name.clone();
    let to_json = serde_json::to_string(&parsed.to_addrs).unwrap();
    let cc_json = serde_json::to_string(&parsed.cc_addrs).unwrap();
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query(r#"INSERT INTO messages (id, tenant_id, mailbox_id, thread_id, message_id, from_addr, from_name, to_addrs, cc_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, raw_r2_key, size_bytes, has_attachments, headers_json, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,'Inbox',false,false,$14,$15,$16,$17,NOW())"#)
                .bind(id).bind(tenant_id).bind(mailbox_id).bind(thread_id).bind(msg_uid)
                .bind(&from_addr).bind(&from_name).bind(&to_json).bind(&cc_json)
                .bind(&parsed.subject).bind(snippet).bind(&parsed.body_text).bind(&parsed.body_html)
                .bind(raw_key).bind(parsed.raw_size as i32).bind(!parsed.attachments.is_empty()).bind(headers_json)
                .execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query(r#"INSERT INTO messages (id, tenant_id, mailbox_id, thread_id, message_id, from_addr, from_name, to_addrs, cc_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, raw_r2_key, size_bytes, has_attachments, headers_json, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"#)
                .bind(id.to_string()).bind(tenant_id.to_string()).bind(mailbox_id.to_string()).bind(thread_id.map(|u| u.to_string()))
                .bind(msg_uid).bind(&from_addr).bind(&from_name).bind(&to_json).bind(&cc_json)
                .bind(&parsed.subject).bind(snippet).bind(&parsed.body_text).bind(&parsed.body_html)
                .bind("Inbox").bind(0).bind(0).bind(raw_key).bind(parsed.raw_size as i32).bind(if parsed.attachments.is_empty(){0}else{1}).bind(headers_json.to_string())
                .bind(Utc::now().to_rfc3339())
                .execute(pool).await?;
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
        });
        let _ = reqwest::Client::new().post(format!("{}/v1/mail/intelligence", ai_url))
            .header("x-internal-token", &state.config.internal_token)
            .json(&payload).timeout(std::time::Duration::from_secs(5)).send().await;
    }
    Ok(())
}
