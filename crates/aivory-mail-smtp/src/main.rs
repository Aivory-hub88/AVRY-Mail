use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpListener};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();
    let smtp_port: u16 = std::env::var("SMTP_INGRESS_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(2525);
    let api_url = std::env::var("AIVORY_MAIL_API_URL").unwrap_or_else(|_| "http://localhost:8095".into());
    let token = std::env::var("INTERNAL_TOKEN").unwrap_or_else(|_| "aivory-internal-dev".into());
    let listener = TcpListener::bind(format!("0.0.0.0:{}", smtp_port)).await?;
    info!("Aivory Mail SMTP ingress listening on :{} → {}", smtp_port, api_url);

    loop {
        let (mut stream, addr) = listener.accept().await?;
        let api_url = api_url.clone();
        let token = token.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_smtp(&mut stream, &api_url, &token).await {
                warn!("smtp {} error: {}", addr, e);
            }
        });
    }
}

async fn handle_smtp(stream: &mut tokio::net::TcpStream, api_url: &str, token: &str) -> anyhow::Result<()> {
    let mut buf = [0u8; 8192];
    stream.write_all(b"220 aivory.mail ESMTP Aivory Mail\r\n").await?;
    let mut from = String::new();
    let mut to = String::new();
    let mut data_mode = false;
    let mut raw: Vec<u8> = Vec::new();

    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 { break; }
        let chunk = &buf[..n];
        if data_mode {
            raw.extend_from_slice(chunk);
            if raw.windows(5).any(|w| w == b"\r\n.\r\n") || raw.ends_with(b"\r\n.\r\n") {
                // strip dot-stuffing terminator
                if let Some(pos) = raw.windows(5).position(|w| w == b"\r\n.\r\n") {
                    raw.truncate(pos);
                }
                // forward to API
                forward_raw(&api_url, token, &from, &to, &raw).await?;
                stream.write_all(b"250 OK queued\r\n").await?;
                raw.clear();
                data_mode = false;
                continue;
            }
            continue;
        }
        let line = String::from_utf8_lossy(chunk).to_string();
        let upper = line.to_uppercase();
        if upper.starts_with("EHLO") || upper.starts_with("HELO") {
            stream.write_all(b"250-aivory.mail\r\n250 STARTTLS\r\n").await?;
        } else if upper.starts_with("MAIL FROM:") {
            from = extract_addr(&line);
            stream.write_all(b"250 OK\r\n").await?;
        } else if upper.starts_with("RCPT TO:") {
            to = extract_addr(&line);
            if recipient_accepted(api_url, token, &to).await {
                stream.write_all(b"250 OK\r\n").await?;
            } else {
                warn!("rejecting RCPT TO <{}> — no such mailbox", to);
                stream.write_all(b"550 5.1.1 User unknown\r\n").await?;
                to.clear();
            }
        } else if upper.starts_with("DATA") {
            if to.is_empty() {
                stream.write_all(b"503 5.5.1 Bad sequence - no valid recipient\r\n").await?;
                continue;
            }
            stream.write_all(b"354 End data with <CR><LF>.<CR><LF>\r\n").await?;
            data_mode = true;
            raw.clear();
        } else if upper.starts_with("QUIT") {
            stream.write_all(b"221 Bye\r\n").await?;
            break;
        } else if upper.starts_with("RSET") {
            from.clear(); to.clear(); raw.clear(); data_mode = false;
            stream.write_all(b"250 OK\r\n").await?;
        } else {
            stream.write_all(b"250 OK\r\n").await?;
        }
    }
    Ok(())
}

fn extract_addr(line: &str) -> String {
    if let Some(s) = line.find('<').and_then(|a| line.find('>').map(|b| (a,b))) {
        line[s.0+1..s.1].trim().to_string()
    } else {
        line.split(':').nth(1).unwrap_or("").trim().trim_matches(|c| c=='<'||c=='>').to_string()
    }
}

/// Asks the API whether a mailbox exists for `to` before accepting it —
/// mirrors real MTA RCPT-time rejection instead of accepting everything and
/// storing orphaned mail. Fails open to false (reject) on any API error so a
/// down API doesn't turn into an open relay.
async fn recipient_accepted(api_url: &str, token: &str, to: &str) -> bool {
    if to.is_empty() { return false; }
    let client = reqwest::Client::new();
    let url = format!("{}/v1/internal/resolve-recipient", api_url);
    let Ok(resp) = client.get(&url)
        .header("x-internal-token", token)
        .query(&[("to", to)])
        .timeout(std::time::Duration::from_secs(5))
        .send().await
    else { return false; };
    let Ok(body) = resp.json::<serde_json::Value>().await else { return false; };
    body.get("data").and_then(|d| d.get("accept")).and_then(|v| v.as_bool()).unwrap_or(false)
}

async fn forward_raw(api_url: &str, token: &str, from: &str, to: &str, raw: &[u8]) -> anyhow::Result<()> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    let b64 = B64.encode(raw);
    let client = reqwest::Client::new();
    let res = client.post(format!("{}/v1/webhooks/inbound", api_url))
        .header("x-internal-token", token)
        .json(&serde_json::json!({"from": from, "to": to, "raw": b64}))
        .send().await?;
    info!("forwarded inbound {} -> {} status {}", from, to, res.status());
    Ok(())
}
