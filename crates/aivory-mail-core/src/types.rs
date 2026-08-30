use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Tenant ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

// ── Domain ──
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DomainStatus {
    Pending,
    Verifying,
    Active,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Domain {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub domain: String,
    pub status: DomainStatus,
    pub dkim_selector: String,
    pub sending_subdomain: Option<String>,
    pub cf_zone_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDomainRequest {
    pub domain: String,
    pub tenant_id: Option<Uuid>,
}

// ── Mailbox ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mailbox {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub domain_id: Uuid,
    pub address: String, // full email
    pub display_name: Option<String>,
    pub is_catch_all: bool,
    pub forward_to: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMailboxRequest {
    pub address: String,
    pub display_name: Option<String>,
    pub is_catch_all: Option<bool>,
    pub forward_to: Option<String>,
}

// ── Message ──
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageFolder {
    Inbox,
    Sent,
    Drafts,
    Spam,
    Trash,
    Archive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub mailbox_id: Uuid,
    pub thread_id: Option<Uuid>,
    pub message_id: String,
    pub from_addr: String,
    pub from_name: Option<String>,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub subject: Option<String>,
    pub snippet: Option<String>,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub folder: MessageFolder,
    pub is_read: bool,
    pub is_starred: bool,
    pub raw_r2_key: Option<String>,
    pub size_bytes: i32,
    pub has_attachments: bool,
    pub created_at: DateTime<Utc>,
    pub headers_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: Uuid,
    pub message_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i32,
    pub r2_key: String,
}

#[derive(Debug, Deserialize)]
pub struct SendRequest {
    pub from: String,
    pub to: Vec<String>,
    pub cc: Option<Vec<String>>,
    pub bcc: Option<Vec<String>>,
    pub subject: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub attachments: Option<Vec<SendAttachment>>,
    pub thread_id: Option<Uuid>,
    pub in_reply_to: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SendAttachment {
    pub filename: String,
    pub content_type: Option<String>,
    pub content_base64: String,
}

// ── Thread ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub mailbox_id: Uuid,
    pub subject: Option<String>,
    pub participant_addrs: Vec<String>,
    pub message_count: i32,
    pub last_message_at: DateTime<Utc>,
    pub has_unread: bool,
}

// ── Intelligence ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntelligenceResult {
    pub summary: String,
    pub intent: String,
    pub urgency: Urgency,
    pub entities: Vec<Entity>,
    pub suggested_actions: Vec<SuggestedAction>,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Urgency { Low, Medium, High, Critical }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub kind: String, // invoice, amount, date, person, org
    pub value: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedAction {
    pub action: String, // create_task, notify_finance, draft_reply, update_crm
    pub label: String,
    pub params: serde_json::Value,
    pub requires_approval: bool,
}

// ── API envelope ──
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self { Self { success: true, data: Some(data), error: None } }
    pub fn err(msg: impl Into<String>) -> Self { Self { success: false, data: None, error: Some(msg.into()) } }
}

#[derive(Debug, Deserialize)]
pub struct Pagination {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub folder: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}
