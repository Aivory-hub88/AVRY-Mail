use anyhow::Result;
use mail_parser::{MessageParser, MimeHeaders};

#[derive(Debug, Clone)]
pub struct ParsedEmail {
    pub message_id: Option<String>,
    pub from_addr: Option<String>,
    pub from_name: Option<String>,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub subject: Option<String>,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub date: Option<i64>,
    pub attachments: Vec<ParsedAttachment>,
    pub headers: Vec<(String, String)>,
    pub raw_size: usize,
}

#[derive(Debug, Clone)]
pub struct ParsedAttachment {
    pub filename: Option<String>,
    pub content_type: String,
    pub data: Vec<u8>,
}

pub fn parse_raw_email(raw: &[u8]) -> Result<ParsedEmail> {
    let msg = MessageParser::default()
        .parse(raw)
        .ok_or_else(|| anyhow::anyhow!("failed to parse email"))?;

    let from_addr = msg.from().and_then(|a| a.first()).and_then(|addr| addr.address.as_deref().map(|s| s.to_string()));
    let from_name = msg.from().and_then(|a| a.first()).and_then(|addr| addr.name.as_deref().map(|s| s.to_string()));
    let to_addrs = msg.to().map(|addrs| addrs.iter().filter_map(|a| a.address.as_deref().map(|s| s.to_string())).collect()).unwrap_or_default();
    let cc_addrs = msg.cc().map(|addrs| addrs.iter().filter_map(|a| a.address.as_deref().map(|s| s.to_string())).collect()).unwrap_or_default();
    let subject = msg.subject().map(|s| s.to_string());
    let message_id = msg.message_id().map(|s| s.to_string());
    let date = msg.date().map(|d| d.to_timestamp());

    let body_text = msg.body_text(0).map(|s| s.to_string());
    let body_html = msg.body_html(0).map(|s| s.to_string());

    let mut attachments = Vec::new();
    for att in msg.attachments() {
        let filename = att.attachment_name().map(|n: &str| n.to_string());
        let ct = att.content_type()
            .map(|ct| format!("{}/{}", ct.c_type, ct.c_subtype.as_deref().unwrap_or("octet-stream")))
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let data = att.contents().to_vec();
        attachments.push(ParsedAttachment { filename, content_type: ct, data });
    }

    let headers: Vec<(String, String)> = msg.headers_raw().map(|(k, v)| (k.to_string(), v.to_string())).collect();

    Ok(ParsedEmail {
        message_id, from_addr, from_name, to_addrs, cc_addrs, subject,
        body_text, body_html, date, attachments, headers, raw_size: raw.len(),
    })
}

pub fn snippet_from_body(text: Option<&str>, html: Option<&str>, max_len: usize) -> String {
    let raw = text.or(html).unwrap_or("");
    let stripped = if html.is_some() && text.is_none() {
        let mut s = String::new();
        let mut in_tag = false;
        for c in raw.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => s.push(c),
                _ => {}
            }
        }
        s
    } else { raw.to_string() };
    let t = stripped.trim().replace('\n', " ").replace('\r', " ");
    if t.len() > max_len {
        // Byte length can land inside a multi-byte UTF-8 character (curly
        // quotes, emoji, etc. are common in real mail) — slicing there
        // panics. Walk back to the nearest char boundary instead.
        let mut end = max_len;
        while end > 0 && !t.is_char_boundary(end) { end -= 1; }
        format!("{}…", &t[..end])
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snippet_truncation_does_not_panic_on_multibyte_boundary() {
        // 159 ASCII chars then a curly quote (3 bytes) straddles byte 160 —
        // this used to panic the whole inbound pipeline on real mail.
        let body = format!("{}’ and some emoji 🎉 too, plus more text to push well past the limit", "a".repeat(159));
        let snippet = snippet_from_body(Some(&body), None, 160);
        assert!(snippet.ends_with('…'));
    }
    #[test]
    fn test_parse_simple() {
        let raw = b"From: Alice <alice@example.com>\r\nTo: bob@example.com\r\nSubject: Hello\r\nMessage-ID: <123@example.com>\r\n\r\nHello world";
        let parsed = parse_raw_email(raw).unwrap();
        assert_eq!(parsed.subject.as_deref(), Some("Hello"));
        assert_eq!(parsed.from_addr.as_deref(), Some("alice@example.com"));
    }
}
