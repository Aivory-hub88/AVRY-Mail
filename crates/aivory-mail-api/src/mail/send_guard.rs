//! Send-path guards that sit in front of `outbound::send_email`:
//!
//! - **Idempotency** — a send carrying an `Idempotency-Key` is recorded
//!   before any transport is contacted, so a retried request (double click,
//!   flaky network, React re-running an updater) returns the first result
//!   instead of emailing the recipient twice.
//! - **Agent send budget** — sends made by an AI agent through MCP are capped
//!   per mailbox per UTC day and can be switched off instance-wide, so a
//!   confused agent loop can never turn into a spam run from a customer's
//!   domain. Sends from the web app are never counted or blocked here.

use aivory_mail_core::types::SendRequest;
use aivory_mail_storage::db::DbPool;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::AppState;

pub const IDEMPOTENCY_HEADER: &str = "idempotency-key";

/// How long a send may stay in flight before a retry stops waiting for it. A
/// process that died mid-send never updates its row, and the mail may already
/// be out, so past this the outcome is reported as unknown instead of retried.
const SEND_LEASE_SECONDS: i64 = 5 * 60;

const DEFAULT_AGENT_DAILY_SEND_LIMIT: i64 = 100;

fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// UUIDs and similar opaque keys: 8–200 chars of `A-Z a-z 0-9 . _ : -`.
pub fn valid_idempotency_key(key: &str) -> bool {
    (8..=200).contains(&key.len())
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
}

/// The header's key: `Ok(None)` when absent, `Err(())` when malformed.
pub fn read_idempotency_key(headers: &axum::http::HeaderMap) -> Result<Option<String>, ()> {
    let Some(raw) = headers.get(IDEMPOTENCY_HEADER) else {
        return Ok(None);
    };
    let key = raw.to_str().map_err(|_| ())?.trim();
    if key.is_empty() {
        return Ok(None);
    }
    if valid_idempotency_key(key) {
        Ok(Some(key.to_string()))
    } else {
        Err(())
    }
}

/// Hash of what the request would actually send. Reusing a key for different
/// content is refused rather than silently replaying the old result.
pub fn request_fingerprint(req: &SendRequest) -> String {
    let canonical = serde_json::to_string(
        &crate::api::mcp_confirmations::canonical_send_payload(req),
    )
    .unwrap_or_default();
    sha256_hex(&canonical)
}

#[derive(Debug, PartialEq, Eq)]
pub enum SendClaim {
    /// This request owns the send; call `finish_send` with its outcome.
    Claimed { attempt_id: String },
    /// The same key already sent this message; return its id, send nothing.
    Replay { message_id: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum SendClaimError {
    /// The key was used before for a different message.
    Conflict,
    /// Another request with this key is sending right now.
    InProgress,
    /// A request with this key started long ago and never reported back.
    OutcomeUnknown,
}

impl SendClaimError {
    pub fn code(&self) -> &'static str {
        match self {
            SendClaimError::Conflict => "idempotency_conflict",
            SendClaimError::InProgress => "send_in_progress",
            SendClaimError::OutcomeUnknown => "send_outcome_unknown",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            SendClaimError::Conflict => {
                "this Idempotency-Key was already used for a different message"
            }
            SendClaimError::InProgress => "a send with this Idempotency-Key is still in progress",
            SendClaimError::OutcomeUnknown => {
                "a send with this Idempotency-Key never finished; check Sent before retrying with a new key"
            }
        }
    }
}

/// What an existing attempt row means for a new request with the same key.
/// Pure so the decision table is testable without a database.
fn decide_existing(
    stored_hash: &str,
    status: &str,
    message_id: Option<String>,
    updated_at: &str,
    request_hash: &str,
    now: DateTime<Utc>,
) -> Result<Option<SendClaim>, SendClaimError> {
    if stored_hash != request_hash {
        return Err(SendClaimError::Conflict);
    }
    match (status, message_id) {
        ("sent", Some(message_id)) => Ok(Some(SendClaim::Replay { message_id })),
        // A failed send may be retried under the same key: signal "take it over".
        ("failed", _) => Ok(None),
        _ => {
            let started = DateTime::parse_from_rfc3339(updated_at)
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or(now);
            if (now - started).num_seconds() > SEND_LEASE_SECONDS {
                Err(SendClaimError::OutcomeUnknown)
            } else {
                Err(SendClaimError::InProgress)
            }
        }
    }
}

/// Take ownership of a send, or learn that it already happened. `caller` is
/// the authenticated identity so keys never collide across users.
pub async fn claim_send(
    state: &Arc<AppState>,
    caller: &str,
    key: &str,
    req: &SendRequest,
) -> anyhow::Result<Result<SendClaim, SendClaimError>> {
    let attempt_id = sha256_hex(&format!("{}\u{0}{}", caller.trim().to_lowercase(), key));
    let request_hash = request_fingerprint(req);
    let now = Utc::now();
    let now_s = now.to_rfc3339();

    let inserted = match &state.db {
        DbPool::Postgres(pool) => sqlx::query(
            "INSERT INTO send_attempts (id, caller, payload_hash, status, message_id, created_at, updated_at) VALUES ($1,$2,$3,'pending',NULL,$4,$4) ON CONFLICT (id) DO NOTHING",
        )
        .bind(&attempt_id)
        .bind(caller)
        .bind(&request_hash)
        .bind(&now_s)
        .execute(pool)
        .await?
        .rows_affected(),
        DbPool::Sqlite(pool) => sqlx::query(
            "INSERT INTO send_attempts (id, caller, payload_hash, status, message_id, created_at, updated_at) VALUES (?,?,?,'pending',NULL,?,?) ON CONFLICT (id) DO NOTHING",
        )
        .bind(&attempt_id)
        .bind(caller)
        .bind(&request_hash)
        .bind(&now_s)
        .bind(&now_s)
        .execute(pool)
        .await?
        .rows_affected(),
    };
    if inserted == 1 {
        return Ok(Ok(SendClaim::Claimed { attempt_id }));
    }

    type Existing = (String, String, Option<String>, String);
    let (stored_hash, status, message_id, updated_at): Existing = match &state.db {
        DbPool::Postgres(pool) => {
            let row = sqlx::query(
                "SELECT payload_hash, status, message_id, updated_at FROM send_attempts WHERE id=$1",
            )
            .bind(&attempt_id)
            .fetch_one(pool)
            .await?;
            (row.get("payload_hash"), row.get("status"), row.get("message_id"), row.get("updated_at"))
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT payload_hash, status, message_id, updated_at FROM send_attempts WHERE id=?",
            )
            .bind(&attempt_id)
            .fetch_one(pool)
            .await?;
            (row.get("payload_hash"), row.get("status"), row.get("message_id"), row.get("updated_at"))
        }
    };
    let decision = decide_existing(&stored_hash, &status, message_id, &updated_at, &request_hash, now);
    match decision {
        Ok(Some(replay)) => Ok(Ok(replay)),
        Err(e) => Ok(Err(e)),
        Ok(None) => {
            // Retry of a failed send: re-claim atomically so two retries racing
            // each other cannot both dispatch.
            let reclaimed = match &state.db {
                DbPool::Postgres(pool) => sqlx::query(
                    "UPDATE send_attempts SET status='pending', updated_at=$2 WHERE id=$1 AND status='failed'",
                )
                .bind(&attempt_id)
                .bind(&now_s)
                .execute(pool)
                .await?
                .rows_affected(),
                DbPool::Sqlite(pool) => sqlx::query(
                    "UPDATE send_attempts SET status='pending', updated_at=? WHERE id=? AND status='failed'",
                )
                .bind(&now_s)
                .bind(&attempt_id)
                .execute(pool)
                .await?
                .rows_affected(),
            };
            if reclaimed == 1 {
                Ok(Ok(SendClaim::Claimed { attempt_id }))
            } else {
                Ok(Err(SendClaimError::InProgress))
            }
        }
    }
}

/// Record how a claimed send ended. Best-effort: a failure here is logged, the
/// send's own result is what the caller reports.
pub async fn finish_send(state: &Arc<AppState>, attempt_id: &str, outcome: Option<Uuid>) {
    let now_s = Utc::now().to_rfc3339();
    let (status, message_id) = match outcome {
        Some(id) => ("sent", Some(id.to_string())),
        None => ("failed", None),
    };
    let result = match &state.db {
        DbPool::Postgres(pool) => sqlx::query(
            "UPDATE send_attempts SET status=$2, message_id=$3, updated_at=$4 WHERE id=$1",
        )
        .bind(attempt_id)
        .bind(status)
        .bind(message_id)
        .bind(&now_s)
        .execute(pool)
        .await
        .map(|_| ()),
        DbPool::Sqlite(pool) => sqlx::query(
            "UPDATE send_attempts SET status=?, message_id=?, updated_at=? WHERE id=?",
        )
        .bind(status)
        .bind(message_id)
        .bind(&now_s)
        .bind(attempt_id)
        .execute(pool)
        .await
        .map(|_| ()),
    };
    if let Err(e) = result {
        tracing::warn!("send_attempts finish failed: {}", e);
    }
}

// ───────────────────────── agent send budget ─────────────────────────

/// `AGENT_SEND_ENABLED=false` stops every agent (MCP) send at once and hides
/// the send tools from `tools/list`. The web app is unaffected.
pub fn agent_send_enabled() -> bool {
    parse_enabled(std::env::var("AGENT_SEND_ENABLED").ok().as_deref())
}

fn parse_enabled(value: Option<&str>) -> bool {
    !matches!(
        value.map(|v| v.trim().to_lowercase()).as_deref(),
        Some("false" | "0" | "no" | "off")
    )
}

/// `AGENT_DAILY_SEND_LIMIT` — agent sends per mailbox per UTC day. Default
/// 100; `0` removes the cap.
pub fn agent_daily_send_limit() -> i64 {
    parse_limit(std::env::var("AGENT_DAILY_SEND_LIMIT").ok().as_deref())
}

fn parse_limit(value: Option<&str>) -> i64 {
    value
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|v| *v >= 0)
        .unwrap_or(DEFAULT_AGENT_DAILY_SEND_LIMIT)
}

#[derive(Debug, PartialEq, Eq)]
pub enum AgentSendRefusal {
    Disabled,
    DailyLimit { limit: i64 },
}

impl AgentSendRefusal {
    pub fn code(&self) -> &'static str {
        match self {
            AgentSendRefusal::Disabled => "agent_send_disabled",
            AgentSendRefusal::DailyLimit { .. } => "daily_send_limit",
        }
    }

    pub fn message(&self) -> String {
        match self {
            AgentSendRefusal::Disabled => {
                "agent_send_disabled: sending by AI agents is turned off on this server. Nothing was sent — tell the user to send it from the Aivory Mail app.".to_string()
            }
            AgentSendRefusal::DailyLimit { limit } => format!(
                "daily_send_limit: this mailbox reached its limit of {} agent-sent emails today (UTC). Nothing was sent — do not retry; tell the user.",
                limit
            ),
        }
    }
}

/// Reserve one agent send for `mailbox_id` today. Call right before dispatch;
/// a reservation is not refunded if the send then fails, because its outcome
/// may be unknown.
pub async fn reserve_agent_send(
    state: &Arc<AppState>,
    mailbox_id: Uuid,
) -> anyhow::Result<Result<(), AgentSendRefusal>> {
    if !agent_send_enabled() {
        return Ok(Err(AgentSendRefusal::Disabled));
    }
    let limit = agent_daily_send_limit();
    if limit == 0 {
        return Ok(Ok(()));
    }
    let day = Utc::now().format("%Y-%m-%d").to_string();
    // The WHERE on the upsert is the cap: concurrent callers increment one row,
    // and once it reaches the limit the statement changes nothing.
    let changed = match &state.db {
        DbPool::Postgres(pool) => sqlx::query(
            "INSERT INTO agent_send_counts (mailbox_id, day, count) VALUES ($1,$2,1) ON CONFLICT (mailbox_id, day) DO UPDATE SET count = agent_send_counts.count + 1 WHERE agent_send_counts.count < $3",
        )
        .bind(mailbox_id.to_string())
        .bind(&day)
        .bind(limit)
        .execute(pool)
        .await?
        .rows_affected(),
        DbPool::Sqlite(pool) => sqlx::query(
            "INSERT INTO agent_send_counts (mailbox_id, day, count) VALUES (?,?,1) ON CONFLICT (mailbox_id, day) DO UPDATE SET count = agent_send_counts.count + 1 WHERE agent_send_counts.count < ?",
        )
        .bind(mailbox_id.to_string())
        .bind(&day)
        .bind(limit)
        .execute(pool)
        .await?
        .rows_affected(),
    };
    if changed == 1 {
        Ok(Ok(()))
    } else {
        Ok(Err(AgentSendRefusal::DailyLimit { limit }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn idempotency_key_shape() {
        assert!(valid_idempotency_key("123e4567-e89b-12d3-a456-426614174000"));
        assert!(valid_idempotency_key("compose:abc.def_1"));
        assert!(!valid_idempotency_key("short"));
        assert!(!valid_idempotency_key("has space in it"));
        assert!(!valid_idempotency_key(&"x".repeat(201)));
    }

    #[test]
    fn header_absent_blank_or_malformed() {
        let mut headers = axum::http::HeaderMap::new();
        assert_eq!(read_idempotency_key(&headers), Ok(None));
        headers.insert(IDEMPOTENCY_HEADER, "   ".parse().unwrap());
        assert_eq!(read_idempotency_key(&headers), Ok(None));
        headers.insert(IDEMPOTENCY_HEADER, "bad key!".parse().unwrap());
        assert_eq!(read_idempotency_key(&headers), Err(()));
        headers.insert(IDEMPOTENCY_HEADER, " 0123456789abcdef ".parse().unwrap());
        assert_eq!(read_idempotency_key(&headers), Ok(Some("0123456789abcdef".into())));
    }

    #[test]
    fn existing_attempt_decisions() {
        let now = Utc::now();
        let fresh = now.to_rfc3339();
        let stale = (now - Duration::seconds(SEND_LEASE_SECONDS + 1)).to_rfc3339();

        assert_eq!(
            decide_existing("h", "sent", Some("m1".into()), &fresh, "other", now),
            Err(SendClaimError::Conflict)
        );
        assert_eq!(
            decide_existing("h", "sent", Some("m1".into()), &fresh, "h", now),
            Ok(Some(SendClaim::Replay { message_id: "m1".into() }))
        );
        assert_eq!(decide_existing("h", "failed", None, &fresh, "h", now), Ok(None));
        assert_eq!(
            decide_existing("h", "pending", None, &fresh, "h", now),
            Err(SendClaimError::InProgress)
        );
        assert_eq!(
            decide_existing("h", "pending", None, &stale, "h", now),
            Err(SendClaimError::OutcomeUnknown)
        );
    }

    #[test]
    fn fingerprint_tracks_content_not_address_case() {
        let req = |to: &str, text: &str| SendRequest {
            from: "me@example.com".into(),
            to: vec![to.into()],
            cc: None,
            bcc: None,
            subject: "s".into(),
            text: Some(text.into()),
            html: None,
            attachments: None,
            thread_id: None,
            in_reply_to: None,
        };
        assert_eq!(
            request_fingerprint(&req("A@x.com", "hi")),
            request_fingerprint(&req("a@x.com", "hi"))
        );
        assert_ne!(
            request_fingerprint(&req("a@x.com", "hi")),
            request_fingerprint(&req("a@x.com", "hello"))
        );
    }

    #[test]
    fn env_parsing() {
        assert!(parse_enabled(None));
        assert!(parse_enabled(Some("true")));
        assert!(!parse_enabled(Some(" FALSE ")));
        assert!(!parse_enabled(Some("0")));
        assert_eq!(parse_limit(None), DEFAULT_AGENT_DAILY_SEND_LIMIT);
        assert_eq!(parse_limit(Some("0")), 0);
        assert_eq!(parse_limit(Some("25")), 25);
        assert_eq!(parse_limit(Some("-3")), DEFAULT_AGENT_DAILY_SEND_LIMIT);
        assert_eq!(parse_limit(Some("lots")), DEFAULT_AGENT_DAILY_SEND_LIMIT);
    }
}
