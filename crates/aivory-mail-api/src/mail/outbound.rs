use aivory_mail_core::{types::SendRequest, routing::validate_send_request};
use anyhow::{Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use lettre::{Message, transport::smtp::authentication::Credentials, AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use tracing::info;
use uuid::Uuid;
use chrono::Utc;
use sqlx::Row;

use crate::api::AppState;
use std::sync::Arc;

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

    let msg = build_message(&req)?;

    // Decide transport: Cloudflare Email Service vs direct SMTP
    let sent_via = if state.config.is_cloudflare() && state.config.cf_api_token.is_some() {
        match send_via_cloudflare(state, &req).await {
            Ok(()) => "cloudflare",
            Err(e) => {
                tracing::warn!("cloudflare send failed, fallback to SMTP: {}", e);
                send_via_smtp(state, msg).await?;
                "smtp-fallback"
            }
        }
    } else {
        send_via_smtp(state, msg).await?;
        "smtp"
    };

    info!("email sent via {} from={} to={:?}", sent_via, req.from, req.to);

    let msg_id = Uuid::new_v4();
    let mailbox_id = resolve_sender_mailbox(state, &req.from).await.unwrap_or(Uuid::nil());
    store_sent_message(state, &msg_id, &mailbox_id, &req).await?;

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

    // Without attachments, use simple body or alternative
    if req.attachments.is_none() || req.attachments.as_ref().unwrap().is_empty() {
        if let Some(h) = html {
            return Ok(builder.multipart(lettre::message::MultiPart::alternative_plain_html(text, h))?);
        } else {
            return Ok(builder.body(text)?);
        }
    }

    // With attachments: multipart/mixed with alternative body + attachments
    // For MVP we send alternative body only and log attachments (full MIME attach requires more lettre wiring)
    // Attachments are stored in R2 and sent via raw API in future; here we warn
    for a in req.attachments.as_ref().unwrap() {
        tracing::warn!("attachment {} will be stored but not inlined in SMTP for MVP (use raw API for full)", a.filename);
    }
    if let Some(h) = html {
        Ok(builder.multipart(lettre::message::MultiPart::alternative_plain_html(text, h))?)
    } else {
        Ok(builder.body(text)?)
    }
}

async fn send_via_smtp(state: &Arc<AppState>, msg: Message) -> Result<()> {
    let host = state.config.smtp_host.clone().unwrap_or_else(|| "localhost".into());
    let port = state.config.smtp_port;
    if host == "localhost" && state.config.smtp_host.is_none() {
        info!("[DEV] SMTP not configured — email would be sent: {:?}", msg.envelope());
        return Ok(());
    }
    if let (Ok(user), Ok(pass)) = (std::env::var("SMTP_USER"), std::env::var("SMTP_PASSWORD")) {
        let creds = Credentials::new(user, pass);
        let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&host)?
            .port(port).credentials(creds).build();
        transport.send(msg).await?;
    } else {
        info!("SMTP sending without auth to {}:{}", host, port);
        let transport: AsyncSmtpTransport<Tokio1Executor> = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
            .port(port).build();
        transport.send(msg).await?;
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
            sqlx::query(r#"INSERT INTO messages (id, tenant_id, mailbox_id, thread_id, message_id, from_addr, to_addrs, cc_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, size_bytes, has_attachments, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'Sent',true,false,0,false,NOW())"#)
                .bind(id).bind(Uuid::nil()).bind(mailbox_id).bind(req.thread_id)
                .bind(format!("<{}@aivory.mail>", id))
                .bind(&req.from).bind(&to_json).bind(&cc_json)
                .bind(&req.subject)
                .bind(req.text.as_deref().unwrap_or("").chars().take(160).collect::<String>())
                .bind(&req.text).bind(&req.html)
                .execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query(r#"INSERT INTO messages (id, tenant_id, mailbox_id, thread_id, message_id, from_addr, to_addrs, cc_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, size_bytes, has_attachments, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"#)
                .bind(id.to_string()).bind(Uuid::nil().to_string()).bind(mailbox_id.to_string()).bind(req.thread_id.map(|u| u.to_string()))
                .bind(format!("<{}@aivory.mail>", id))
                .bind(&req.from).bind(&to_json).bind(&cc_json)
                .bind(&req.subject)
                .bind(req.text.as_deref().unwrap_or("").chars().take(160).collect::<String>())
                .bind(&req.text).bind(&req.html)
                .bind("Sent").bind(1).bind(0).bind(0).bind(0)
                .bind(Utc::now().to_rfc3339())
                .execute(pool).await?;
        }
    }
    Ok(())
}
