use aivory_mail_core::{types::SendRequest, routing::validate_send_request, validation::extract_domain};
use anyhow::{Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use lettre::{Message, transport::smtp::authentication::Credentials, AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use tracing::info;
use uuid::Uuid;
use chrono::Utc;
use sqlx::Row;

use crate::api::AppState;
use std::sync::Arc;

struct SenderDomainAuth {
    dkim_selector: String,
    dkim_private_key: String,
}

/// Require the `from` address's domain to be verified (Active) with a DKIM
/// key on file before allowing a send — stops spoofing/using arbitrary
/// unverified domains and guarantees every outbound message can be signed.
async fn require_verified_sender_domain(state: &Arc<AppState>, from: &str) -> Result<SenderDomainAuth> {
    let domain = extract_domain(from).ok_or_else(|| anyhow::anyhow!("invalid from address"))?;
    let found: Option<(String, String, Option<String>)> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("SELECT status, dkim_selector, dkim_private_key FROM domains WHERE lower(domain)=$1")
                .bind(&domain).fetch_optional(pool).await?
                .map(|row| (row.get("status"), row.get("dkim_selector"), row.get("dkim_private_key")))
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("SELECT status, dkim_selector, dkim_private_key FROM domains WHERE lower(domain)=?")
                .bind(&domain).fetch_optional(pool).await?
                .map(|row| (row.get("status"), row.get("dkim_selector"), row.get("dkim_private_key")))
        }
    };
    let Some((status, dkim_selector, dkim_private_key)) = found else { bail!("domain {} is not registered in Aivory Mail", domain) };
    if status != "Active" {
        bail!("domain {} is not verified yet — add the DNS records and verify before sending", domain);
    }
    let Some(dkim_private_key) = dkim_private_key else { bail!("domain {} has no DKIM key on file", domain) };
    Ok(SenderDomainAuth { dkim_selector, dkim_private_key })
}

pub async fn send_email(state: &Arc<AppState>, req: SendRequest) -> Result<Uuid> {
    validate_send_request(&req)?;

    // Validate attachment sizes
    if let Some(atts) = &req.attachments {
        let mut total: usize = 0;
        for a in atts {
            let decoded = B64.decode(a.content_base64.trim())?;
            if decoded.len() > 10 * 1024 * 1024 { bail!("attachment {} exceeds 10MB", a.filename); }
            total += decoded.len();
        }
        if total > 20 * 1024 * 1024 { bail!("combined attachments exceed 20MB"); }
    }

    let sender_auth = require_verified_sender_domain(state, &req.from).await?;
    let msg = build_message(&req)?;
    let envelope = msg.envelope().clone();
    let raw = msg.formatted();
    let domain = extract_domain(&req.from).unwrap_or_default();
    let dkim_header = crate::mail::dkim::sign(&sender_auth.dkim_private_key, &sender_auth.dkim_selector, &domain, &raw)
        .map_err(|e| { tracing::error!("dkim sign failed for {}: {}", domain, e); e })?;
    let signed_raw = [dkim_header.as_bytes(), raw.as_slice()].concat();

    // Decide transport: Cloudflare Email Service vs direct SMTP
    let sent_via = if state.config.is_cloudflare() && state.config.cf_api_token.is_some() {
        match send_via_cloudflare(state, &req).await {
            Ok(()) => "cloudflare",
            Err(e) => {
                tracing::warn!("cloudflare send failed, fallback to SMTP: {}", e);
                send_via_smtp(state, &envelope, &signed_raw).await?;
                "smtp-fallback"
            }
        }
    } else {
        send_via_smtp(state, &envelope, &signed_raw).await?;
        "smtp"
    };

    info!("email sent via {} from={} to={:?}", sent_via, req.from, req.to);

    let msg_id = Uuid::new_v4();
    let mailbox_id = resolve_sender_mailbox(state, &req.from).await.unwrap_or(Uuid::nil());
    store_sent_message(state, &msg_id, &mailbox_id, &req).await?;
    // Also graph_remember sent mail (outbox) — same tenant
    {
        let body = req.text.clone().or(req.html.clone()).unwrap_or_default();
        let subj = req.subject.clone();
        let mid = msg_id.to_string();
        let tenant = mailbox_id.to_string(); // fallback; real tenant should be from mailbox tenant_id
        tokio::spawn(async move {
            let agent_type = std::env::var("COGNEE_AGENT_TYPE").unwrap_or_else(|_| "mail_ops".into());
            let _ = crate::mail::cognee_client::remember_email(&tenant, &agent_type, &subj, &body, &mid).await;
        });
    }

    Ok(msg_id)
}

fn build_message(req: &SendRequest) -> Result<Message> {
    let mut builder = Message::builder()
        .from(req.from.parse()?)
        .subject(req.subject.clone());
    for to in &req.to { builder = builder.to(to.parse()?); }
    if let Some(cc) = &req.cc { for c in cc { builder = builder.cc(c.parse()?); } }
    if let Some(bcc) = &req.bcc { for b in bcc { builder = builder.bcc(b.parse()?); } }

    let text = req.text.clone().unwrap_or_default();
    let html = req.html.clone();
    let has_attachments = req.attachments.as_ref().map(|a| !a.is_empty()).unwrap_or(false);

    if !has_attachments {
        if let Some(h) = html {
            return Ok(builder.multipart(lettre::message::MultiPart::alternative_plain_html(text.clone(), h))?);
        } else {
            return Ok(builder.body(text)?);
        }
    }

    // With attachments: build mixed multipart containing alternative body + each file
    let atts = req.attachments.as_ref().unwrap();
    let alt = if let Some(h) = html.clone() {
        lettre::message::MultiPart::alternative_plain_html(text.clone(), h)
    } else {
        lettre::message::MultiPart::alternative()
            .singlepart(lettre::message::SinglePart::plain(text.clone()))
    };

    // Start mixed with alternative as first part by converting alt to raw and adding attachments
    // Workaround: lettre MultiPart builders don't support pushing; we construct parts manually.
    // Build attachments as SinglePart with base64 already handled by lettre transport.
    let mut mixed = lettre::message::MultiPart::mixed().build();
    // Instead of builder dance, we create a manual multipart/mixed via Message::new + body
    // Simplest that actually sends files: use mail-builder semantics via raw construction.
    // We collect attachment parts
    let mut attachment_parts: Vec<lettre::message::SinglePart> = Vec::new();
    for a in atts {
        let data = B64.decode(a.content_base64.trim())?;
        let ct_str = a.content_type.clone().unwrap_or_else(|| "application/octet-stream".into());
        let (top, sub) = ct_str.split_once('/').unwrap_or(("application", "octet-stream"));
        let mime: lettre::message::header::ContentType = format!("{}/{}", top, sub).parse().unwrap_or(lettre::message::header::ContentType::TEXT_PLAIN);
        let part = lettre::message::Attachment::new(a.filename.clone()).body(data, mime);
        attachment_parts.push(part);
    }

    // Build final: if lettre mixed supports from+alt, use mixed; fallback to builder.multipart with attachments
    // Use lettre::message::MultiPart::mixed().multipart(alt).singlepart(...)
    // Since lettre 0.11 mixed().singlepart expects SinglePart, we need to convert.
    // Approach: create mixed that contains the alt (as one part) plus each attachment.
    // We do this by creating a custom multipart: lettre expects MultiPart to be built via .multipart/.singlepart chaining.
    // Easiest: use builder.multipart(mixed_from_parts) where we construct via Message::multipart after assembling bytes.
    // For production, we construct raw MIME string ourselves and let lettre parse? Instead, use mail-send crate fallback for complex.
    // Here we do the correct lettre way: SinglePart for body + attachments via mixed builder pattern.
    // Build a mixed containing the alt rendered as bytes inside a SinglePart wrapper.
    // Simpler: if attachments exist, send as mixed with plain body + attachments (no alternative) to guarantee delivery.
    if html.is_some() {
        // mixed with alternative inside: use the lettre helper for mixed+alternative
        // Create mixed where first part is the alternative multipart encoded as a SinglePart? Not ideal.
        // We fallback to sending mixed with plain+html alternatives flattened + attachments.
        let plain_part = lettre::message::SinglePart::plain(text.clone());
        let mut mixed_builder = lettre::message::MultiPart::mixed().singlepart(plain_part);
        if let Some(h) = html {
            let html_part = lettre::message::SinglePart::html(h);
            mixed_builder = mixed_builder.singlepart(html_part);
        }
        for ap in attachment_parts {
            mixed_builder = mixed_builder.singlepart(ap);
        }
        return Ok(builder.multipart(mixed_builder)?);
    } else {
        let mut mixed_builder = lettre::message::MultiPart::mixed().singlepart(lettre::message::SinglePart::plain(text));
        for ap in attachment_parts {
            mixed_builder = mixed_builder.singlepart(ap);
        }
        return Ok(builder.multipart(mixed_builder)?);
    }
}

async fn send_via_smtp(state: &Arc<AppState>, envelope: &lettre::address::Envelope, raw: &[u8]) -> Result<()> {
    let host = state.config.smtp_host.clone().unwrap_or_else(|| "localhost".into());
    let port = state.config.smtp_port;
    let is_prod = std::env::var("RUST_ENV").map(|v| v=="production").unwrap_or(false) || std::env::var("ENV").map(|v| v=="production").unwrap_or(false);
    if host == "localhost" && state.config.smtp_host.is_none() {
        if is_prod {
            anyhow::bail!("SMTP_HOST not configured in production — refusing to silently drop mail to {:?}", envelope);
        }
        info!("[DEV] SMTP not configured — email would be sent: {:?}", envelope);
        return Ok(());
    }
    if let (Ok(user), Ok(pass)) = (std::env::var("SMTP_USER"), std::env::var("SMTP_PASSWORD")) {
        let creds = Credentials::new(user, pass);
        let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&host)?
            .port(port).credentials(creds).build();
        transport.send_raw(envelope, raw).await?;
    } else {
        info!("SMTP sending without auth to {}:{}", host, port);
        let transport: AsyncSmtpTransport<Tokio1Executor> = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
            .port(port).build();
        transport.send_raw(envelope, raw).await?;
    }
    Ok(())
}

async fn send_via_cloudflare(state: &Arc<AppState>, req: &SendRequest) -> Result<()> {
    let token = state.config.cf_api_token.as_ref().unwrap();
    let zone_id = state.config.cf_zone_id.clone().unwrap_or_default();
    if zone_id.is_empty() { anyhow::bail!("CF_ZONE_ID not set"); }
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "from": req.from, "to": req.to, "subject": req.subject, "text": req.text, "html": req.html,
    });
    let resp = client.post(format!("https://api.cloudflare.com/client/v4/zones/{}/email/sending/send", zone_id))
        .bearer_auth(token).json(&payload).send().await?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("cloudflare send failed: {}", body);
    }
    Ok(())
}

async fn resolve_sender_mailbox(state: &Arc<AppState>, from: &str) -> Option<Uuid> {
    let norm = from.trim().to_lowercase();
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT id FROM mailboxes WHERE lower(address)=$1 LIMIT 1")
                .bind(&norm).fetch_optional(pool).await.ok()??;
            Some(row.get::<Uuid,_>("id"))
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id FROM mailboxes WHERE lower(address)=? LIMIT 1")
                .bind(&norm).fetch_optional(pool).await.ok()??;
            let s: String = row.get("id");
            Uuid::parse_str(&s).ok()
        }
    }
}

async fn store_sent_message(state: &Arc<AppState>, id: &Uuid, mailbox_id: &Uuid, req: &SendRequest) -> Result<()> {
    let to_json = serde_json::to_string(&req.to).unwrap();
    let cc_json = req.cc.as_ref().map(|c| serde_json::to_string(c).unwrap()).unwrap_or_else(|| "[]".into());
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            let has_att = req.attachments.as_ref().map(|a| !a.is_empty()).unwrap_or(false);
            sqlx::query(r#"INSERT INTO messages (id, tenant_id, mailbox_id, thread_id, message_id, from_addr, to_addrs, cc_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, size_bytes, has_attachments, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'Sent',true,false,0,$13,NOW())"#)
                .bind(id).bind(Uuid::nil()).bind(mailbox_id).bind(req.thread_id)
                .bind(format!("<{}@aivory.mail>", id))
                .bind(&req.from).bind(&to_json).bind(&cc_json)
                .bind(&req.subject)
                .bind(req.text.as_deref().unwrap_or("").chars().take(160).collect::<String>())
                .bind(&req.text).bind(&req.html)
                .execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let has_att_sqlite = req.attachments.as_ref().map(|a| !a.is_empty()).unwrap_or(false);
            sqlx::query(r#"INSERT INTO messages (id, tenant_id, mailbox_id, thread_id, message_id, from_addr, to_addrs, cc_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, size_bytes, has_attachments, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"#)
                .bind(id.to_string()).bind(Uuid::nil().to_string()).bind(mailbox_id.to_string()).bind(req.thread_id.map(|u| u.to_string()))
                .bind(format!("<{}@aivory.mail>", id))
                .bind(&req.from).bind(&to_json).bind(&cc_json)
                .bind(&req.subject)
                .bind(req.text.as_deref().unwrap_or("").chars().take(160).collect::<String>())
                .bind(&req.text).bind(&req.html)
                .bind("Sent").bind(1).bind(0).bind(0).bind(if has_att_sqlite {1} else {0})
                .bind(Utc::now().to_rfc3339())
                .execute(pool).await?;
        }
    }
    Ok(())
}
