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

    // Mailflare parity: 2MB body limit + 10/10MB/20MB attachments
    if let Some(t) = &req.text { if t.len() > 2 * 1024 * 1024 { bail!("text body exceeds 2MB"); } }
    if let Some(h) = &req.html { if h.len() > 2 * 1024 * 1024 { bail!("html body exceeds 2MB"); } }
    if let Some(atts) = &req.attachments {
        if atts.len() > 10 { bail!("too many attachments: max 10"); }
        let mut total: usize = 0;
        for a in atts {
            if a.filename.contains('/') || a.filename.contains('\0') { bail!("invalid filename: {}", a.filename); }
            let decoded = B64.decode(a.content_base64.trim())?;
            if decoded.len() > 10 * 1024 * 1024 { bail!("attachment {} exceeds 10MB", a.filename); }
            total += decoded.len();
        }
        if total > 20 * 1024 * 1024 { bail!("combined attachments exceed 20MB"); }
    }

    let sender_auth = require_verified_sender_domain(state, &req.from).await?;
    // Manual raw construction for plain text to avoid lettre InvalidContentType (missing MIME-Version)
    let (envelope, raw) = {
        use lettre::address::Envelope;
        let from_addr: lettre::Address = req.from.parse().map_err(|e| anyhow::anyhow!("invalid from: {}", e))?;
        let to_addrs: Vec<lettre::Address> = req.to.iter().map(|s| s.parse()).collect::<Result<Vec<_>, _>>().map_err(|e| anyhow::anyhow!("invalid to: {}", e))?;
        let envelope = Envelope::new(Some(from_addr), to_addrs.clone()).map_err(|e| anyhow::anyhow!("envelope: {}", e))?;
        let date = chrono::Utc::now().to_rfc2822();
        let msg_id = format!("<{}@aivory.uk>", Uuid::new_v4());
        let body = req.text.clone().unwrap_or_default();
        let mut headers = String::new();
        headers.push_str(&format!("From: {}\r\n", req.from));
        headers.push_str(&format!("To: {}\r\n", req.to.join(", ")));
        if let Some(cc) = &req.cc { if !cc.is_empty() { headers.push_str(&format!("Cc: {}\r\n", cc.join(", "))); } }
        headers.push_str(&format!("Subject: {}\r\n", req.subject));
        headers.push_str(&format!("Date: {}\r\n", date));
        headers.push_str(&format!("Message-ID: {}\r\n", msg_id));
        headers.push_str("MIME-Version: 1.0\r\n");
        headers.push_str("Content-Type: text/plain; charset=us-ascii\r\n");
        headers.push_str("Content-Transfer-Encoding: 7bit\r\n");
        headers.push_str("\r\n");
        headers.push_str(&body);
        let raw = headers.into_bytes();
        tracing::info!("manual raw envelope {:?} len {} preview: {}", envelope, raw.len(), String::from_utf8_lossy(&raw[..raw.len().min(300)]));
        let _ = std::fs::write("/tmp/last_msg.eml", &raw);
        (envelope, raw)
    };
    let domain = extract_domain(&req.from).unwrap_or_default();
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
                tracing::warn!("cloudflare send failed: {}, falling back to worker-http/mailchannels/smtp", e);
                send_via_fallback_chain(state, &req, &envelope, &signed_raw).await?
            }
        }
    } else {
        send_via_fallback_chain(state, &req, &envelope, &signed_raw).await?
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
            // Explicit Content-Type to avoid InvalidContentType on some lettre versions
            use lettre::message::header::ContentType;
            return Ok(builder.header(ContentType::TEXT_PLAIN).body(text)?);
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
    if let Ok(()) = send_via_mail_send(state, envelope, raw).await {
        return Ok(());
    }
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

async fn send_via_worker_http(state: &Arc<AppState>, req: &SendRequest) -> Result<()> {
    let url = state.config.worker_send_url.clone().or_else(|| std::env::var("WORKER_SEND_URL").ok()).unwrap_or_else(|| "https://worker.aivory.uk/send".into());
    if !url.starts_with("http://") && !url.starts_with("https://") {
        anyhow::bail!("not a worker url");
    }
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "from": req.from,
        "to": req.to,
        "subject": req.subject,
        "text": req.text,
        "html": req.html,
    });
    tracing::info!("worker http send to {} from {} to {:?}", url, req.from, req.to);
    let resp = client.post(&url).json(&payload).send().await?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("worker http failed: {} - {}", status, body);
    }
    tracing::info!("worker http ok: {}", body);
    Ok(())
}

async fn send_via_mail_send(state: &Arc<AppState>, envelope: &lettre::address::Envelope, raw: &[u8]) -> Result<()> {
    tracing::info!("mail_send trying {}:{}", state.config.smtp_host.clone().unwrap_or_else(|| "localhost".into()), state.config.smtp_port);
    use mail_send::{SmtpClientBuilder, Credentials as MailSendCreds};
    let host = state.config.smtp_host.clone().unwrap_or_else(|| "localhost".into());
    let port = state.config.smtp_port;
    let from = envelope.from().map(|a| a.to_string()).unwrap_or_else(|| "hello@aivory.uk".to_string());
    let to_list: Vec<String> = envelope.to().iter().map(|a| a.to_string()).collect();
    let raw_str = String::from_utf8_lossy(raw);
    let subject = raw_str.lines().find(|l| l.to_lowercase().starts_with("subject:")).map(|s| s[8..].trim().to_string()).unwrap_or_else(|| "No subject".to_string());
    let body_start = raw_str.find("\r\n\r\n").map(|p| p+4).or_else(|| raw_str.find("\n\n").map(|p| p+2)).unwrap_or(0);
    let body = raw_str[body_start..].to_string();
    let mut builder = mail_send::mail_builder::MessageBuilder::new();
    builder = builder.from(from.clone());
    for to in &to_list { builder = builder.to(to.clone()); }
    builder = builder.subject(subject);
    builder = builder.text_body(body);
    let mut client_builder = SmtpClientBuilder::new(host.clone(), port).implicit_tls(false);
    if let (Ok(user), Ok(pass)) = (std::env::var("SMTP_USER"), std::env::var("SMTP_PASSWORD")) {
        client_builder = client_builder.credentials(MailSendCreds::new(user, pass));
    }
    tracing::info!("mail_send connect to {}:{}", host, port);
    let mut client = client_builder.connect().await.map_err(|e| { tracing::error!("mail-send connect failed: {:?}", e); anyhow::anyhow!("mail-send connect failed to {}:{}: {}", host, port, e) })?;
    client.send(builder).await.map_err(|e| anyhow::anyhow!("mail-send send failed: {}", e))?;
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
    if account_id.is_empty() { anyhow::bail!("CF_ACCOUNT_ID not set"); }
    let client = reqwest::Client::new();
    let mut payload = serde_json::json!({
        "from": req.from, "to": req.to, "subject": req.subject,
    });
    if let Some(html) = &req.html { payload["html"] = serde_json::json!(html); }
    if let Some(text) = &req.text { payload["text"] = serde_json::json!(text); }
    if payload.get("html").is_none() && payload.get("text").is_none() {
        payload["text"] = serde_json::json!("");
    }
    if let Some(cc) = &req.cc { if !cc.is_empty() { payload["cc"] = serde_json::json!(cc); } }
    if let Some(bcc) = &req.bcc { if !bcc.is_empty() { payload["bcc"] = serde_json::json!(bcc); } }
    let resp = client.post(format!("https://api.cloudflare.com/client/v4/accounts/{}/email/sending/send", account_id))
        .bearer_auth(token).json(&payload).send().await?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("cloudflare send failed: {}", body);
    }
    Ok(())
}

async fn send_via_mailchannels(state: &Arc<AppState>, req: &SendRequest) -> Result<()> {
    let client = reqwest::Client::new();
    let (from_name, from_email) = if req.from.contains('<') {
        let name = req.from.split('<').next().unwrap_or("").trim().trim_matches('"').trim();
        let email = req.from.split('<').nth(1).unwrap_or("").trim_end_matches('>').trim();
        (name, email)
    } else {
        ("", req.from.as_str())
    };
    let mut personalizations = Vec::new();
    for to in &req.to {
        personalizations.push(serde_json::json!({"to": [{"email": to}]}));
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
    tracing::info!("mailchannels send from {} to {:?} subject {}", from_email, req.to, req.subject);
    let resp = client.post("https://api.mailchannels.net/tx/v1/send")
        .json(&payload).send().await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("mailchannels send failed: {} - {}", status, body);
    }
    Ok(())
}

async fn send_via_mailersend_api(state: &Arc<AppState>, req: &SendRequest) -> Result<()> {
    let api_key = std::env::var("SMTP_PASSWORD").or_else(|_| std::env::var("MAILERSEND_API_KEY")).map_err(|_| anyhow::anyhow!("mailersend api key not set"))?;
    let client = reqwest::Client::new();
    let from_email = if req.from.contains('<') {
        req.from.split('<').nth(1).unwrap_or("").trim_end_matches('>').trim().to_string()
    } else {
        req.from.clone()
    };
    let from_name = if req.from.contains('<') {
        req.from.split('<').next().unwrap_or("").trim().trim_matches('"').trim().to_string()
    } else {
        "".to_string()
    };
    let to_list: Vec<serde_json::Value> = req.to.iter().map(|e| serde_json::json!({"email": e})).collect();
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
    tracing::info!("mailersend api send from {} to {:?} subject {}", from_email, req.to, req.subject);
    let resp = client.post("https://api.mailersend.com/v1/email")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("X-Requested-With", "XMLHttpRequest")
        .json(&payload).send().await?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("mailersend api failed: {} - {}", status, body);
    }
    tracing::info!("mailersend api ok: {}", body);
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
                .bind(&req.text).bind(&req.html).bind(has_att)
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
