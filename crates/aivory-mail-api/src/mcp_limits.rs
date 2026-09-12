use aivory_mail_core::types::SendRequest;
use axum::http::StatusCode;
use serde_json::Value;

/// Immutable bounds for the capability-backed MCP surface.
///
/// These values are deliberately owned by the MCP boundary rather than by a
/// provider adapter. A model or remote client must not be able to turn a
/// provider default into an unbounded API request.
pub struct McpLimits;

impl McpLimits {
    pub const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
    pub const MAX_QUERY_BYTES: usize = 64 * 1024;
    pub const MAX_FOLDER_BYTES: usize = 1 * 1024;
    pub const MAX_THREAD_ID_BYTES: usize = 128;
    pub const MAX_SEARCH_LIMIT: u64 = 50;
    pub const MAX_THREAD_BUDGET: u64 = 20_000;
    pub const MAX_THREAD_MESSAGES: usize = 100;
    pub const MAX_THREAD_FIELD_BYTES: usize = 20_000;
    pub const MAX_KNOWLEDGE_BUDGET: u64 = 20_000;
    pub const MAX_RESULT_BYTES: usize = 8 * 1024 * 1024;
    pub const MAX_RECIPIENTS: usize = 100;
    pub const MAX_ADDRESS_BYTES: usize = 1 * 1024;
    pub const MAX_SUBJECT_BYTES: usize = 64 * 1024;
    pub const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
    pub const MAX_ATTACHMENTS: usize = 10;
    pub const MAX_ATTACHMENT_FILENAME_BYTES: usize = 4 * 1024;
    pub const MAX_ATTACHMENT_BASE64_BYTES: usize = 14 * 1024 * 1024;
    pub const MAX_TOTAL_ATTACHMENT_BASE64_BYTES: usize = 28 * 1024 * 1024;
}

pub fn required_string<'a>(
    args: &'a Value,
    name: &str,
    maximum_bytes: usize,
) -> Result<&'a str, StatusCode> {
    let value = args
        .get(name)
        .and_then(Value::as_str)
        .ok_or(StatusCode::BAD_REQUEST)?;
    validate_string(value, maximum_bytes).map(|_| value)
}

pub fn optional_string<'a>(
    args: &'a Value,
    name: &str,
    maximum_bytes: usize,
) -> Result<Option<&'a str>, StatusCode> {
    let Some(value) = args.get(name) else {
        return Ok(None);
    };
    let value = value.as_str().ok_or(StatusCode::BAD_REQUEST)?;
    validate_string(value, maximum_bytes).map(|_| Some(value))
}

pub fn bounded_integer(
    args: &Value,
    name: &str,
    default: u64,
    maximum: u64,
) -> Result<usize, StatusCode> {
    let value = match args.get(name) {
        None => default,
        Some(value) => value.as_u64().ok_or(StatusCode::BAD_REQUEST)?,
    };
    if value == 0 || value > maximum {
        return Err(StatusCode::BAD_REQUEST);
    }
    usize::try_from(value).map_err(|_| StatusCode::BAD_REQUEST)
}

pub fn validate_string(value: &str, maximum_bytes: usize) -> Result<(), StatusCode> {
    if value.trim().is_empty() || value.as_bytes().len() > maximum_bytes {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

pub fn validate_send_request(request: &SendRequest) -> Result<(), StatusCode> {
    let recipient_count = request.to.len()
        + request.cc.as_ref().map_or(0, Vec::len)
        + request.bcc.as_ref().map_or(0, Vec::len);
    if recipient_count == 0 || recipient_count > McpLimits::MAX_RECIPIENTS {
        return Err(StatusCode::BAD_REQUEST);
    }

    for address in std::iter::once(&request.from)
        .chain(request.to.iter())
        .chain(request.cc.iter().flatten())
        .chain(request.bcc.iter().flatten())
    {
        validate_string(address, McpLimits::MAX_ADDRESS_BYTES)?;
    }
    validate_string(&request.subject, McpLimits::MAX_SUBJECT_BYTES)?;
    if request.text.is_none() && request.html.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }
    for body in [&request.text, &request.html].into_iter().flatten() {
        if body.as_bytes().len() > McpLimits::MAX_BODY_BYTES {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let attachments = request.attachments.as_deref().unwrap_or_default();
    if attachments.len() > McpLimits::MAX_ATTACHMENTS {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut total_attachment_bytes = 0usize;
    for attachment in attachments {
        if attachment.filename.as_bytes().len() > McpLimits::MAX_ATTACHMENT_FILENAME_BYTES
            || attachment.filename.contains('/')
            || attachment.filename.contains('\\')
            || attachment.filename.contains('\0')
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        let encoded_bytes = attachment.content_base64.as_bytes().len();
        if encoded_bytes > McpLimits::MAX_ATTACHMENT_BASE64_BYTES {
            return Err(StatusCode::BAD_REQUEST);
        }
        total_attachment_bytes = total_attachment_bytes
            .checked_add(encoded_bytes)
            .ok_or(StatusCode::BAD_REQUEST)?;
    }
    if total_attachment_bytes > McpLimits::MAX_TOTAL_ATTACHMENT_BASE64_BYTES {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

pub fn validate_rpc_result(value: &Value) -> Result<(), StatusCode> {
    let serialized = serde_json::to_vec(value).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if serialized.len() > McpLimits::MAX_RESULT_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{bounded_integer, validate_send_request, McpLimits};
    use aivory_mail_core::types::SendRequest;
    use serde_json::json;

    fn request() -> SendRequest {
        SendRequest {
            from: "sender@example.com".to_string(),
            to: vec!["recipient@example.com".to_string()],
            cc: None,
            bcc: None,
            subject: "subject".to_string(),
            text: Some("body".to_string()),
            html: None,
            attachments: None,
            thread_id: None,
            in_reply_to: None,
        }
    }

    #[test]
    fn bounds_reject_zero_and_overflow_instead_of_silently_clamping() {
        assert_eq!(
            bounded_integer(&json!({}), "limit", 10, McpLimits::MAX_SEARCH_LIMIT).unwrap(),
            10
        );
        assert!(bounded_integer(&json!({"limit": 0}), "limit", 10, 50).is_err());
        assert!(bounded_integer(&json!({"limit": 51}), "limit", 10, 50).is_err());
        assert!(bounded_integer(&json!({"limit": "10"}), "limit", 10, 50).is_err());
    }

    #[test]
    fn send_limits_cover_recipients_content_and_attachment_names() {
        assert!(validate_send_request(&request()).is_ok());
        let mut changed = request();
        changed.to = (0..=McpLimits::MAX_RECIPIENTS)
            .map(|index| format!("recipient{index}@example.com"))
            .collect();
        assert!(validate_send_request(&changed).is_err());
        let mut changed = request();
        changed.text = Some("x".repeat(McpLimits::MAX_BODY_BYTES + 1));
        assert!(validate_send_request(&changed).is_err());
    }
}
