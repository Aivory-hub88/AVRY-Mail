use aivory_mail_core::{
    routing::validate_send_request, types::SendRequest, validation::extract_domain,
};
use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chrono::Utc;
use lettre::{
    transport::smtp::authentication::Credentials, AsyncSmtpTransport, AsyncTransport, Message,
    Tokio1Executor,
};
use sqlx::Row;
use tracing::info;
use uuid::Uuid;

use crate::api::{execution_context::ExecutionContext, AppState};
use std::sync::Arc;

/// Require the `from` address's domain to be verified (Active) with a DKIM
/// key on file before allowing a send. DKIM signing is currently disabled in
/// the transport layer, but verification remains mandatory to prevent spoofing.
async fn require_verified_sender_domain(state: &Arc<AppState>, from: &str) -> Result<()> {
    let domain = extract_domain(from).ok_or_else(|| anyhow::anyhow!("invalid from address"))?;
    let found: Option<(String, Option<String>)> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("SELECT status, dkim_private_key FROM domains WHERE lower(domain)=$1")
                .bind(&domain)
                .fetch_optional(pool)
                .await?
                .map(|row| (row.get("status"), row.get("dkim_private_key")))
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("SELECT status, dkim_private_key FROM domains WHERE lower(domain)=?")
                .bind(&domain)
                .fetch_optional(pool)
                .await?
                .map(|row| (row.get("status"), row.get("dkim_private_key")))
        }
    };
    let Some((status, dkim_private_key)) = found else {
        bail!("domain {} is not registered in Aivory Mail", domain)
    };
    if status != "Active" {
        bail!(
            "domain {} is not verified yet — add the DNS records and verify before sending",
            domain
        );
    }
    if dkim_private_key.is_none() {
        bail!("domain {} has no DKIM key on file", domain)
    }
    Ok(())
}

pub async fn authorize_sender(
    state: &Arc<AppState>,
    headers: &axum::http::HeaderMap,
    req: &SendRequest,
) -> Result<()> {
    validate_send_request(req)?;
    let from = req.from.trim().to_lowercase();
    let caller = crate::api::authz::authenticated_email(state, headers)
        .map_err(|_| anyhow::anyhow!("missing or invalid user session"))?;
    if !crate::api::authz::is_admin(state, &caller).await && from != caller {
        bail!("sender mailbox is not owned by the authenticated user");
    }
    validate_thread_ownership(state, &from, req.thread_id).await?;
    Ok(())
}

/// A supplied thread id is part of the mailbox boundary, not just a UI hint.
/// Validate it before any transport is contacted and again from send_email so
/// internal callers such as the reply endpoint receive the same protection.
async fn validate_thread_ownership(
    state: &Arc<AppState>,
    from: &str,
    thread_id: Option<Uuid>,
) -> Result<()> {
    let Some(thread_id) = thread_id else {
        return Ok(());
    };
    let mailbox_id = resolve_sender_mailbox(state, from)
        .await
        .ok_or_else(|| anyhow::anyhow!("sender mailbox does not exist"))?;
    let owned = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("SELECT 1 FROM threads WHERE id=$1 AND mailbox_id=$2")
                .bind(thread_id)
                .bind(mailbox_id)
                .fetch_optional(pool)
                .await?
                .is_some()
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("SELECT 1 FROM threads WHERE id=? AND mailbox_id=?")
                .bind(thread_id.to_string())
                .bind(mailbox_id.to_string())
                .fetch_optional(pool)
                .await?
                .is_some()
        }
    };
    if !owned {
        bail!("thread does not belong to the sender mailbox");
    }
    Ok(())
}

pub async fn send_email_with_context(
    state: &Arc<AppState>,
    req: SendRequest,
    context: &ExecutionContext,
) -> Result<Uuid> {
    let from = req.from.trim().to_lowercase();
    let belongs = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => sqlx::query(
            "SELECT 1 FROM mailboxes WHERE id=$1 AND tenant_id=$2 AND lower(address)=$3",
        )
        .bind(context.mailbox_id)
        .bind(context.tenant_id)
        .bind(&from)
        .fetch_optional(pool)
        .await?
        .is_some(),
        aivory_mail_storage::db::DbPool::Sqlite(pool) => sqlx::query(
            "SELECT 1 FROM mailboxes WHERE id=? AND tenant_id=? AND lower(address)=?",
        )
        .bind(context.mailbox_id.to_string())
        .bind(context.tenant_id.to_string())
        .bind(&from)
        .fetch_optional(pool)
        .await?
        .is_some(),
    };
    if !belongs {
        bail!("sender mailbox is outside the execution context");
    }
    send_email(state, req).await
}

pub async fn send_email(state: &Arc<AppState>, req: SendRequest) -> Result<Uuid> {
    validate_send_request(&req)?;
    // Mailflare parity: 2MB body limit + 10/10MB/20MB attachments
    if let Some(t) = &req.text {
        if t.len() > 2 * 1024 * 1024 {
            bail!("text body exceeds 2MB");
        }
    }
    if let Some(h) = &req.html {
        if h.len() > 2 * 1024 * 1024 {
            bail!("html body exceeds 2MB");
        }
    }
    if let Some(atts) = &req.attachments {
        if atts.len() > 10 {
            bail!("too many attachments: max 10");
        }
        let mut total: usize = 0;
        for a in atts {
            if a.filename.contains('/') || a.filename.contains('\0') {
                bail!("invalid filename: {}", a.filename);
            }
            let decoded = B64.decode(a.content_base64.trim())?;
            if decoded.len() > 10 * 1024 * 1024 {
                bail!("attachment {} exceeds 10MB", a.filename);
            }
            total += decoded.len();
        }
        if total > 20 * 1024 * 1024 {
            bail!("combined attachments exceed 20MB");
        }
    }

    validate_thread_ownership(state, &req.from, req.thread_id).await?;
    require_verified_sender_domain(state, &req.from).await?;
    // Manual raw construction for plain text to avoid lettre InvalidContentType (missing MIME-Version)
    let (envelope, raw) = {
        use lettre::address::Envelope;
        let from_addr: lettre::Address = req
            .from
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid from: {}", e))?;
        let to_addrs: Vec<lettre::Address> = req
            .to
            .iter()
            .chain(req.cc.as_ref().into_iter().flatten())
            .chain(req.bcc.as_ref().into_iter().flatten())
            .map(|s| s.parse())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("invalid recipient: {}", e))?;
        let envelope = Envelope::new(Some(from_addr), to_addrs)
            .map_err(|e| anyhow::anyhow!("envelope: {}", e))?;
        let date = chrono::Utc::now().to_rfc2822();
        let msg_id = format!("<{}@aivory.uk>", Uuid::new_v4());
        let body = req.text.clone().unwrap_or_default();
        let mut headers = String::new();
        headers.push_str(&format!("From: {}\r\n", req.from));
        headers.push_str(&format!("To: {}\r\n", req.to.join(", ")));
        if let Some(cc) = &req.cc {
            if !cc.is_empty() {
                headers.push_str(&format!("Cc: {}\r\n", cc.join(", ")));
            }
        }
        headers.push_str(&format!("Subject: {}\r\n", req.subject));
        headers.push_str(&format!("Date: {}\r\n", date));
        headers.push_str(&format!("Message-ID: {}\r\n", msg_id));
        headers.push_str("MIME-Version: 1.0\r\n");
        headers.push_str("Content-Type: text/plain; charset=us-ascii\r\n");
        headers.push_str("Content-Transfer-Encoding: 7bit\r\n");
        headers.push_str("\r\n");
        headers.push_str(&body);
        let raw = headers.into_bytes();
        // Never log or persist message content. The raw bytes remain in-memory
        // only for the selected transport and are discarded after sending.
        (envelope, raw)
    };
    let signed_raw = raw.clone();
    tracing::info!("DKIM disabled, using raw len {}", signed_raw.len());

    // Decide transport. Cloudflare Email Sending is now enabled + DNS
    // verified for aivory.uk, so it's the primary path — no per-message cost
    // or MailerSend dependency. worker-http/mailchannels (MailChannels ended
    // its free Cloudflare-Workers relay in 2024, so that path is legacy/dead
    // for us) and SMTP (MailerSend) remain only as a fallback chain for the
    // rare case the Cloudflare API call itself fails.
    let sent_via = if state.config.is_cloudflare() && state.config.cf_api_token.is_some() {
        match send_via_cloudflare(state, &req).await {
            Ok(()) => "cloudflare",
            Err(e) => {
                tracing::warn!(
                    "cloudflare send failed: {}, falling back to worker-http/mailchannels/smtp",
                    e
                );
                send_via_fallback_chain(state, &req, &envelope, &signed_raw).await?
            }
        }
    } else {
        send_via_fallback_chain(state, &req, &envelope, &signed_raw).await?
    };

    info!(
        "email sent via {} recipient_count={}",
        sent_via,
        req.to.len()
    );

    let msg_id = Uuid::new_v4();
    let mailbox_id = resolve_sender_mailbox(state, &req.from)
        .await
        .ok_or_else(|| anyhow::anyhow!("sender mailbox does not exist"))?;
    let tenant_id = resolve_sender_tenant(state, &mailbox_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("sender mailbox tenant does not exist"))?;
    store_sent_message(state, &msg_id, &tenant_id, &mailbox_id, &req).await?;
    // store_sent_message only wrote the has_attachments flag — without this,
    // a Sent message showed a paperclip but there was nothing behind it: no
    // attachment row, no file in storage, no way to ever open what you sent.
    if let Some(atts) = &req.attachments {
        for a in atts {
            let att_id = Uuid::new_v4();
            let ct = a
                .content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".into());
            let data = B64.decode(a.content_base64.trim())?;
            let key = format!("attachments/{}/{}/{}", msg_id, att_id, a.filename);
            state.store.put(&key, data.clone(), &ct).await?;
            crate::mail::inbound::insert_attachment(
                state,
                &att_id,
                &msg_id,
                &a.filename,
                &ct,
                data.len() as i32,
                &key,
            )
            .await?;
        }
    }
    // Also graph_remember sent mail (outbox) — same tenant
    {
        let body = req.text.clone().or(req.html.clone()).unwrap_or_default();
        let subj = req.subject.clone();
        let mid = msg_id.to_string();
        let tenant = tenant_id.to_string();
        tokio::spawn(async move {
            let agent_type =
                std::env::var("COGNEE_AGENT_TYPE").unwrap_or_else(|_| "mail_ops".into());
            let _ = crate::mail::cognee_client::remember_email(
                &tenant,
                &agent_type,
                &subj,
                &body,
                &mid,
            )
            .await;
        });
    }

    Ok(msg_id)
}

/// worker-http -> mailchannels -> SMTP (MailerSend). Used when Cloudflare
/// Email Sending isn't configured, or as the fallback when it errors.
async fn send_via_fallback_chain(
    state: &Arc<AppState>,
    req: &SendRequest,
    envelope: &lettre::address::Envelope,
    signed_raw: &[u8],
) -> Result<&'static str> {
    if let Ok(()) = send_via_worker_http(state, req).await {
        return Ok("worker-http");
    }
    if std::env::var("MAILCHANNELS_DISABLE").is_err() {
        if let Ok(()) = send_via_mailchannels(state, req).await {
            return Ok("mailchannels");
        }
    }
    send_via_smtp(state, envelope, signed_raw).await?;
    Ok("smtp")
}

/// Legacy lettre MIME builder retained for future transport support; current
/// delivery uses raw MIME plus the configured provider chain.
#[allow(dead_code)]
fn build_message(req: &SendRequest) -> Result<Message> {
    let mut builder = Message::builder()
        .from(req.from.parse()?)
        .subject(req.subject.clone());
    for to in &req.to {
        builder = builder.to(to.parse()?);
    }
    if let Some(cc) = &req.cc {
        for c in cc {
            builder = builder.cc(c.parse()?);
        }
    }
    if let Some(bcc) = &req.bcc {
        for b in bcc {
            builder = builder.bcc(b.parse()?);
        }
    }

    let text = req.text.clone().unwrap_or_default();
    let html = req.html.clone();
    let has_attachments = req
        .attachments
        .as_ref()
        .map(|a| !a.is_empty())
        .unwrap_or(false);

    if !has_attachments {
        if let Some(h) = html {
            return Ok(
                builder.multipart(lettre::message::MultiPart::alternative_plain_html(
                    text.clone(),
                    h,
                ))?,
            );
        } else {
            // Explicit Content-Type to avoid InvalidContentType on some lettre versions
            use lettre::message::header::ContentType;
            return Ok(builder.header(ContentType::TEXT_PLAIN).body(text)?);
        }
    }

    // With attachments: build mixed multipart containing alternative body + each file
    let atts = req.attachments.as_ref().unwrap();
    // Build attachments as SinglePart with base64 already handled by lettre transport.
    let mut attachment_parts: Vec<lettre::message::SinglePart> = Vec::new();
    for a in atts {
        let data = B64.decode(a.content_base64.trim())?;
        let ct_str = a
            .content_type
            .clone()
            .unwrap_or_else(|| "application/octet-stream".into());
        let (top, sub) = ct_str
            .split_once('/')
            .unwrap_or(("application", "octet-stream"));
        let mime: lettre::message::header::ContentType = format!("{}/{}", top, sub)
            .parse()
            .unwrap_or(lettre::message::header::ContentType::TEXT_PLAIN);
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
        let mut mixed_builder = lettre::message::MultiPart::mixed()
            .singlepart(lettre::message::SinglePart::plain(text));
        for ap in attachment_parts {
            mixed_builder = mixed_builder.singlepart(ap);
        }
        return Ok(builder.multipart(mixed_builder)?);
    }
}

async fn send_via_smtp(
    state: &Arc<AppState>,
    envelope: &lettre::address::Envelope,
    raw: &[u8],
) -> Result<()> {
    if let Ok(()) = send_via_mail_send(state, envelope, raw).await {
        return Ok(());
    }
    let host = state
        .config
        .smtp_host
        .clone()
        .unwrap_or_else(|| "localhost".into());
    let port = state.config.smtp_port;
    let is_prod = std::env::var("RUST_ENV")
        .map(|v| v == "production")
        .unwrap_or(false)
        || std::env::var("ENV")
            .map(|v| v == "production")
            .unwrap_or(false);
    if host == "localhost" && state.config.smtp_host.is_none() {
        if is_prod {
            anyhow::bail!(
                "SMTP_HOST not configured in production — refusing to silently drop mail to {:?}",
                envelope
            );
        }
        info!(
            "[DEV] SMTP not configured — email would be sent: {:?}",
            envelope
        );
        return Ok(());
    }
    if let (Ok(user), Ok(pass)) = (std::env::var("SMTP_USER"), std::env::var("SMTP_PASSWORD")) {
        let creds = Credentials::new(user, pass);
        let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&host)?
            .port(port)
            .credentials(creds)
            .build();
        transport.send_raw(envelope, raw).await?;
    } else {
        info!("SMTP sending without auth to {}:{}", host, port);
        let transport: AsyncSmtpTransport<Tokio1Executor> =
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
                .port(port)
                .build();
        transport.send_raw(envelope, raw).await?;
    }
    Ok(())
}

async fn send_via_worker_http(state: &Arc<AppState>, req: &SendRequest) -> Result<()> {
    let url = state
        .config
        .worker_send_url
        .clone()
        .or_else(|| std::env::var("WORKER_SEND_URL").ok())
        .unwrap_or_else(|| "https://worker.aivory.uk/send".into());
    if !url.starts_with("http://") && !url.starts_with("https://") {
        anyhow::bail!("not a worker url");
    }
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "from": req.from,
        "to": req.to,
        "cc": req.cc,
        "bcc": req.bcc,
        "subject": req.subject,
        "text": req.text,
        "html": req.html,
        "attachments": req.attachments,
    });
    tracing::info!("worker http send attempt recipient_count={}", req.to.len());
    let resp = client
        .post(&url)
        .header("x-internal-token", &state.config.internal_token)
        .json(&payload)
        .send()
        .await?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("worker http failed: {} - {}", status, body);
    }
    tracing::info!("worker http send accepted");
    Ok(())
}

async fn send_via_mail_send(
    state: &Arc<AppState>,
    envelope: &lettre::address::Envelope,
    raw: &[u8],
) -> Result<()> {
    tracing::info!(
        "mail_send trying {}:{}",
        state
            .config
            .smtp_host
            .clone()
            .unwrap_or_else(|| "localhost".into()),
        state.config.smtp_port
    );
    use mail_send::{Credentials as MailSendCreds, SmtpClientBuilder};
    let host = state
        .config
        .smtp_host
        .clone()
        .unwrap_or_else(|| "localhost".into());
    let port = state.config.smtp_port;
    let from = envelope
        .from()
        .map(|a| a.to_string())
        .unwrap_or_else(|| "hello@aivory.uk".to_string());
    let to_list: Vec<String> = envelope.to().iter().map(|a| a.to_string()).collect();
    let raw_str = String::from_utf8_lossy(raw);
    let subject = raw_str
        .lines()
        .find(|l| l.to_lowercase().starts_with("subject:"))
        .map(|s| s[8..].trim().to_string())
        .unwrap_or_else(|| "No subject".to_string());
    let body_start = raw_str
        .find("\r\n\r\n")
        .map(|p| p + 4)
        .or_else(|| raw_str.find("\n\n").map(|p| p + 2))
        .unwrap_or(0);
    let body = raw_str[body_start..].to_string();
    let mut builder = mail_send::mail_builder::MessageBuilder::new();
    builder = builder.from(from.clone());
    for to in &to_list {
        builder = builder.to(to.clone());
    }
    builder = builder.subject(subject);
    builder = builder.text_body(body);
    let mut client_builder = SmtpClientBuilder::new(host.clone(), port).implicit_tls(false);
    if let (Ok(user), Ok(pass)) = (std::env::var("SMTP_USER"), std::env::var("SMTP_PASSWORD")) {
        client_builder = client_builder.credentials(MailSendCreds::new(user, pass));
    }
    tracing::info!("mail_send connect to {}:{}", host, port);
    let mut client = client_builder.connect().await.map_err(|e| {
        tracing::error!("mail-send connect failed: {:?}", e);
        anyhow::anyhow!("mail-send connect failed to {}:{}: {}", host, port, e)
    })?;
    client
        .send(builder)
        .await
        .map_err(|e| anyhow::anyhow!("mail-send send failed: {}", e))?;
    tracing::info!("mail-send via {}:{} succeeded", host, port);
    Ok(())
}

async fn send_via_cloudflare(state: &Arc<AppState>, req: &SendRequest) -> Result<()> {
    // Cloudflare Email Sending's REST API lives under the *account*, not the
    // zone (`/accounts/{account_id}/email/sending/send`) — the previous
    // `/zones/{zone_id}/...` URL doesn't exist, which is why every call
    // 500'd with email.sending.error.invalid_request_schema and silently
    // fell through to the SMTP/MailerSend fallback on every send.
    let token = state.config.cf_api_token.as_ref().unwrap();
    let account_id = state.config.cf_account_id.clone().unwrap_or_default();
    if account_id.is_empty() {
        anyhow::bail!("CF_ACCOUNT_ID not set");
    }
    let client = reqwest::Client::new();
    let mut payload = serde_json::json!({
        "from": req.from, "to": req.to, "subject": req.subject,
    });
    if let Some(html) = &req.html {
        payload["html"] = serde_json::json!(html);
    }
    if let Some(text) = &req.text {
        payload["text"] = serde_json::json!(text);
    }
    if payload.get("html").is_none() && payload.get("text").is_none() {
        payload["text"] = serde_json::json!("");
    }
    if let Some(cc) = &req.cc {
        if !cc.is_empty() {
            payload["cc"] = serde_json::json!(cc);
        }
    }
    if let Some(bcc) = &req.bcc {
        if !bcc.is_empty() {
            payload["bcc"] = serde_json::json!(bcc);
        }
    }
    // Cloudflare's own field names differ from ours (content/type vs
    // content_base64/content_type) — without this translation attachments
    // were silently dropped on every send that went out via Cloudflare
    // (the primary transport): the recipient got the email text but never
    // the file, with nothing in our logs to say so since the API call still
    // reported success.
    if let Some(atts) = &req.attachments {
        if !atts.is_empty() {
            payload["attachments"] = serde_json::json!(atts.iter().map(|a| serde_json::json!({
                "content": a.content_base64,
                "filename": a.filename,
                "type": a.content_type.clone().unwrap_or_else(|| "application/octet-stream".into()),
                "disposition": "attachment",
            })).collect::<Vec<_>>());
        }
    }
    let resp = client
        .post(format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/email/sending/send",
            account_id
        ))
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("cloudflare send failed: {}", body);
    }
    Ok(())
}

async fn send_via_mailchannels(_state: &Arc<AppState>, req: &SendRequest) -> Result<()> {
    let client = reqwest::Client::new();
    let (from_name, from_email) = if req.from.contains('<') {
        let name = req
            .from
            .split('<')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"')
            .trim();
        let email = req
            .from
            .split('<')
            .nth(1)
            .unwrap_or("")
            .trim_end_matches('>')
            .trim();
        (name, email)
    } else {
        ("", req.from.as_str())
    };
    let mut personalizations = Vec::new();
    for to in &req.to {
        let mut recipient = serde_json::json!({"to": [{"email": to}]});
        if let Some(cc) = &req.cc {
            if !cc.is_empty() {
                recipient["cc"] = serde_json::json!(cc
                    .iter()
                    .map(|email| serde_json::json!({"email": email}))
                    .collect::<Vec<_>>());
            }
        }
        if let Some(bcc) = &req.bcc {
            if !bcc.is_empty() {
                recipient["bcc"] = serde_json::json!(bcc
                    .iter()
                    .map(|email| serde_json::json!({"email": email}))
                    .collect::<Vec<_>>());
            }
        }
        personalizations.push(recipient);
    }
    let mut content = Vec::new();
    if let Some(html) = &req.html {
        content.push(serde_json::json!({"type": "text/html", "value": html}));
        if let Some(text) = &req.text {
            content.push(serde_json::json!({"type": "text/plain", "value": text}));
        }
    } else if let Some(text) = &req.text {
        content.push(serde_json::json!({"type": "text/plain", "value": text}));
    } else {
        content.push(serde_json::json!({"type": "text/plain", "value": ""}));
    }
    let payload = serde_json::json!({
        "personalizations": personalizations,
        "from": {"email": from_email, "name": if from_name.is_empty() { from_email } else { from_name }},
        "subject": req.subject,
        "content": content,
    });
    tracing::info!(
        "mailchannels send attempt recipient_count={}",
        req.to.len()
    );
    let resp = client
        .post("https://api.mailchannels.net/tx/v1/send")
        .json(&payload)
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("mailchannels send failed: {} - {}", status, body);
    }
    Ok(())
}

/// Legacy MailerSend REST fallback retained for future provider re-enablement.
#[allow(dead_code)]
async fn send_via_mailersend_api(_state: &Arc<AppState>, req: &SendRequest) -> Result<()> {
    let api_key = std::env::var("SMTP_PASSWORD")
        .or_else(|_| std::env::var("MAILERSEND_API_KEY"))
        .map_err(|_| anyhow::anyhow!("mailersend api key not set"))?;
    let client = reqwest::Client::new();
    let from_email = if req.from.contains('<') {
        req.from
            .split('<')
            .nth(1)
            .unwrap_or("")
            .trim_end_matches('>')
            .trim()
            .to_string()
    } else {
        req.from.clone()
    };
    let from_name = if req.from.contains('<') {
        req.from
            .split('<')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"')
            .trim()
            .to_string()
    } else {
        "".to_string()
    };
    let to_list: Vec<serde_json::Value> = req
        .to
        .iter()
        .map(|e| serde_json::json!({"email": e}))
        .collect();
    let mut payload = serde_json::json!({
        "from": {"email": from_email, "name": if from_name.is_empty() { "Aivory Mail".to_string() } else { from_name }},
        "to": to_list,
        "subject": req.subject,
    });
    if let Some(text) = &req.text {
        payload["text"] = serde_json::Value::String(text.clone());
    }
    if let Some(html) = &req.html {
        payload["html"] = serde_json::Value::String(html.clone());
    }
    tracing::info!(
        "mailersend api send attempt recipient_count={}",
        req.to.len()
    );
    let resp = client
        .post("https://api.mailersend.com/v1/email")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("X-Requested-With", "XMLHttpRequest")
        .json(&payload)
        .send()
        .await?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("mailersend api failed: {} - {}", status, body);
    }
    tracing::info!("mailersend api send accepted");
    Ok(())
}

async fn resolve_sender_mailbox(state: &Arc<AppState>, from: &str) -> Option<Uuid> {
    let norm = from.trim().to_lowercase();
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT id FROM mailboxes WHERE lower(address)=$1 LIMIT 1")
                .bind(&norm)
                .fetch_optional(pool)
                .await
                .ok()??;
            Some(row.get::<Uuid, _>("id"))
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT id FROM mailboxes WHERE lower(address)=? LIMIT 1")
                .bind(&norm)
                .fetch_optional(pool)
                .await
                .ok()??;
            let s: String = row.get("id");
            Uuid::parse_str(&s).ok()
        }
    }
}

async fn resolve_sender_tenant(state: &Arc<AppState>, mailbox_id: &Uuid) -> Option<Uuid> {
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            let row = sqlx::query("SELECT tenant_id FROM mailboxes WHERE id=$1 LIMIT 1")
                .bind(mailbox_id)
                .fetch_optional(pool)
                .await
                .ok()??;
            Some(row.get::<Uuid, _>("tenant_id"))
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT tenant_id FROM mailboxes WHERE id=? LIMIT 1")
                .bind(mailbox_id.to_string())
                .fetch_optional(pool)
                .await
                .ok()??;
            Uuid::parse_str(&row.get::<String, _>("tenant_id")).ok()
        }
    }
}

async fn store_sent_message(
    state: &Arc<AppState>,
    id: &Uuid,
    tenant_id: &Uuid,
    mailbox_id: &Uuid,
    req: &SendRequest,
) -> Result<()> {
    let to_json = serde_json::to_string(&req.to).unwrap();
    let cc_json = req
        .cc
        .as_ref()
        .map(|c| serde_json::to_string(c).unwrap())
        .unwrap_or_else(|| "[]".into());
    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            let has_att = req
                .attachments
                .as_ref()
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            sqlx::query(r#"INSERT INTO messages (id, tenant_id, mailbox_id, thread_id, message_id, from_addr, to_addrs, cc_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, size_bytes, has_attachments, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'Sent',true,false,0,$13,NOW())"#)
                .bind(id).bind(tenant_id).bind(mailbox_id).bind(req.thread_id)
                .bind(format!("<{}@aivory.mail>", id))
                .bind(&req.from).bind(&to_json).bind(&cc_json)
                .bind(&req.subject)
                .bind(req.text.as_deref().unwrap_or("").chars().take(160).collect::<String>())
                .bind(&req.text).bind(&req.html).bind(has_att)
                .execute(pool).await?;
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            let has_att_sqlite = req
                .attachments
                .as_ref()
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            sqlx::query(r#"INSERT INTO messages (id, tenant_id, mailbox_id, thread_id, message_id, from_addr, to_addrs, cc_addrs, subject, snippet, body_text, body_html, folder, is_read, is_starred, size_bytes, has_attachments, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"#)
                .bind(id.to_string()).bind(tenant_id.to_string()).bind(mailbox_id.to_string()).bind(req.thread_id.map(|u| u.to_string()))
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
    // Dovecot IMAP mirror: sent mail lands in .Sent/cur as Seen.
    {
        let st = state.clone();
        let mb = *mailbox_id;
        let mid = *id;
        let from = req.from.clone();
        let to = req.to.join(", ");
        let subj = req.subject.clone();
        let txt = req.text.clone().unwrap_or_default();
        tokio::spawn(async move {
            if let Some(addr) = crate::mail::maildir::mailbox_address(&st, &mb).await {
                let date = Utc::now().to_rfc2822();
                let rawm = format!("From: {from}\r\nTo: {to}\r\nSubject: {subj}\r\nDate: {date}\r\nMessage-ID: <{mid}@aivory.mail>\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{txt}");
                match crate::mail::maildir::deliver_raw(&addr, "Sent", true, rawm.as_bytes()).await
                {
                    Ok(rel) => {
                        let _ = crate::mail::maildir::record_maildir_file(&st, &mid, &rel).await;
                    }
                    Err(e) => tracing::warn!("maildir deliver (sent) failed: {}", e),
                }
            }
        });
    }
    Ok(())
}
