use aivory_mail_core::{routing::validate_send_request, types::SendRequest};
use aivory_mail_storage::db::DbPool;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

use crate::mcp_limits;
use super::{execution_context::ExecutionContext, AppState};

pub const SEND_MAIL_ACTION: &str = "send_mail";
const DEFAULT_LIFETIME_SECONDS: i64 = 5 * 60;
const MAX_LIFETIME_SECONDS: i64 = 15 * 60;

#[derive(Debug, Deserialize)]
pub struct IssueSendConfirmationRequest {
    pub tenant_id: String,
    pub mailbox_id: String,
    pub caller_id: String,
    pub payload: SendRequest,
    pub expires_in_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendConfirmationMetadata {
    pub id: String,
    pub tenant_id: String,
    pub mailbox_id: String,
    pub caller_id: String,
    pub action: String,
    pub payload_hash: String,
    pub expires_at: String,
    pub issued_at: String,
    pub used_at: Option<String>,
}

fn parse_uuid(value: &str) -> Result<Uuid, axum::http::StatusCode> {
    Uuid::parse_str(value.trim()).map_err(|_| axum::http::StatusCode::BAD_REQUEST)
}

/// Build a deterministic, identity-complete representation of the proposed
/// send. The confirmation binds the actual operation, not a model-provided
/// mailbox selector. Address casing and surrounding whitespace are normalized;
/// message content and attachment bytes remain exact.
pub fn canonical_send_payload(request: &SendRequest) -> serde_json::Value {
    serde_json::json!({
        "action": SEND_MAIL_ACTION,
        "from": request.from.trim().to_lowercase(),
        "to": request.to.iter().map(|value| value.trim().to_lowercase()).collect::<Vec<_>>(),
        "cc": request.cc.as_ref().map(|values| values.iter().map(|value| value.trim().to_lowercase()).collect::<Vec<_>>()).unwrap_or_default(),
        "bcc": request.bcc.as_ref().map(|values| values.iter().map(|value| value.trim().to_lowercase()).collect::<Vec<_>>()).unwrap_or_default(),
        "subject": request.subject.as_str(),
        "text": request.text.as_deref(),
        "html": request.html.as_deref(),
        "attachments": request.attachments.as_ref(),
        "thread_id": request.thread_id.map(|value| value.to_string()),
        "in_reply_to": request.in_reply_to.as_deref(),
    })
}

pub fn payload_hash(request: &SendRequest) -> Result<String, axum::http::StatusCode> {
    let bytes = serde_json::to_vec(&canonical_send_payload(request))
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

async fn validate_mailbox_sender(
    state: &Arc<AppState>,
    tenant_id: Uuid,
    mailbox_id: Uuid,
    from: &str,
) -> Result<(), axum::http::StatusCode> {
    let address = from.trim().to_lowercase();
    let valid = match &state.db {
        DbPool::Postgres(pool) => sqlx::query(
            "SELECT 1 FROM mailboxes WHERE id=$1 AND tenant_id=$2 AND lower(address)=$3",
        )
        .bind(mailbox_id)
        .bind(tenant_id)
        .bind(address)
        .fetch_optional(pool)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .is_some(),
        DbPool::Sqlite(pool) => sqlx::query(
            "SELECT 1 FROM mailboxes WHERE id=? AND tenant_id=? AND lower(address)=?",
        )
        .bind(mailbox_id.to_string())
        .bind(tenant_id.to_string())
        .bind(address)
        .fetch_optional(pool)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .is_some(),
    };
    if valid {
        Ok(())
    } else {
        Err(axum::http::StatusCode::FORBIDDEN)
    }
}

pub async fn issue_send_confirmation(
    state: &Arc<AppState>,
    request: IssueSendConfirmationRequest,
) -> Result<SendConfirmationMetadata, axum::http::StatusCode> {
    let tenant_id = parse_uuid(&request.tenant_id)?;
    let mailbox_id = parse_uuid(&request.mailbox_id)?;
    let caller_id = request.caller_id.trim().to_string();
    if caller_id.is_empty() || caller_id.len() > 200 {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }
    validate_send_request(&request.payload)
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    mcp_limits::validate_send_request(&request.payload)?;
    validate_mailbox_sender(state, tenant_id, mailbox_id, &request.payload.from).await?;
    let hash = payload_hash(&request.payload)?;
    let lifetime = request
        .expires_in_seconds
        .unwrap_or(DEFAULT_LIFETIME_SECONDS);
    if !(1..=MAX_LIFETIME_SECONDS).contains(&lifetime) {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }
    let id = Uuid::new_v4();
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(lifetime);

    match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO mcp_send_confirmations (id, tenant_id, mailbox_id, caller_id, action, payload_hash, expires_at, issued_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(id)
                .bind(tenant_id)
                .bind(mailbox_id)
                .bind(&caller_id)
                .bind(SEND_MAIL_ACTION)
                .bind(&hash)
                .bind(expires_at)
                .bind(issued_at)
                .execute(pool)
                .await
                .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO mcp_send_confirmations (id, tenant_id, mailbox_id, caller_id, action, payload_hash, expires_at, issued_at) VALUES (?,?,?,?,?,?,?,?)")
                .bind(id.to_string())
                .bind(tenant_id.to_string())
                .bind(mailbox_id.to_string())
                .bind(&caller_id)
                .bind(SEND_MAIL_ACTION)
                .bind(&hash)
                .bind(expires_at.to_rfc3339())
                .bind(issued_at.to_rfc3339())
                .execute(pool)
                .await
                .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }

    Ok(SendConfirmationMetadata {
        id: id.to_string(),
        tenant_id: tenant_id.to_string(),
        mailbox_id: mailbox_id.to_string(),
        caller_id,
        action: SEND_MAIL_ACTION.to_string(),
        payload_hash: hash,
        expires_at: expires_at.to_rfc3339(),
        issued_at: issued_at.to_rfc3339(),
        used_at: None,
    })
}

/// Atomically consume a confirmation. The UPDATE predicate is the replay
/// boundary: a concurrent request can consume at most one row, and payload,
/// mailbox, caller, action, and expiry are all checked before consumption.
pub async fn consume_send_confirmation(
    state: &Arc<AppState>,
    context: &ExecutionContext,
    confirmation_id: &str,
    request: &SendRequest,
) -> Result<(), axum::http::StatusCode> {
    let id = parse_uuid(confirmation_id).map_err(|_| axum::http::StatusCode::CONFLICT)?;
    let hash = payload_hash(request)?;
    let changed = match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE mcp_send_confirmations SET used_at=NOW() WHERE id=$1 AND tenant_id=$2 AND mailbox_id=$3 AND caller_id=$4 AND action=$5 AND payload_hash=$6 AND used_at IS NULL AND expires_at > NOW()")
                .bind(id)
                .bind(context.tenant_id)
                .bind(context.mailbox_id)
                .bind(&context.caller_id)
                .bind(SEND_MAIL_ACTION)
                .bind(hash)
                .execute(pool)
                .await
                .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
                .rows_affected()
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE mcp_send_confirmations SET used_at=? WHERE id=? AND tenant_id=? AND mailbox_id=? AND caller_id=? AND action=? AND payload_hash=? AND used_at IS NULL AND expires_at > ?")
                .bind(Utc::now().to_rfc3339())
                .bind(id.to_string())
                .bind(context.tenant_id.to_string())
                .bind(context.mailbox_id.to_string())
                .bind(&context.caller_id)
                .bind(SEND_MAIL_ACTION)
                .bind(hash)
                .bind(Utc::now().to_rfc3339())
                .execute(pool)
                .await
                .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
                .rows_affected()
        }
    };
    if changed == 1 {
        Ok(())
    } else {
        Err(axum::http::StatusCode::CONFLICT)
    }
}

#[cfg(test)]
mod tests {
    use super::{canonical_send_payload, payload_hash};
    use aivory_mail_core::types::{SendAttachment, SendRequest};

    fn request(subject: &str) -> SendRequest {
        SendRequest {
            from: "  mailbox@example.com ".to_string(),
            to: vec!["Recipient@example.com".to_string()],
            cc: None,
            bcc: None,
            subject: subject.to_string(),
            text: Some("body".to_string()),
            html: None,
            attachments: Some(vec![SendAttachment {
                filename: "a.txt".to_string(),
                content_type: Some("text/plain".to_string()),
                content_base64: "Ym9keQ==".to_string(),
            }]),
            thread_id: None,
            in_reply_to: None,
        }
    }

    #[test]
    fn canonical_payload_normalizes_addresses_but_covers_content() {
        let value = canonical_send_payload(&request("subject"));
        assert_eq!(value["from"], "mailbox@example.com");
        assert_eq!(value["to"][0], "recipient@example.com");
        assert_eq!(value["attachments"][0]["filename"], "a.txt");
        assert_ne!(payload_hash(&request("one")).unwrap(), payload_hash(&request("two")).unwrap());
    }

    #[test]
    fn payload_hash_changes_when_recipient_or_body_changes() {
        let original = payload_hash(&request("subject")).unwrap();
        let mut changed = request("subject");
        changed.to.push("other@example.com".to_string());
        assert_ne!(original, payload_hash(&changed).unwrap());
        changed = request("subject");
        changed.text = Some("changed".to_string());
        assert_ne!(original, payload_hash(&changed).unwrap());
    }
}
