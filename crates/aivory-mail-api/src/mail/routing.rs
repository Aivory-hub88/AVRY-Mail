use crate::api::AppState;
use anyhow::Result;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RecipientResolution {
    pub accept: bool,
    pub mailbox_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub reason: String,
}

fn reject(reason: &str) -> RecipientResolution {
    RecipientResolution { accept: false, mailbox_id: None, tenant_id: None, reason: reason.into() }
}

/// DB-backed recipient resolution used both by the SMTP-time RCPT TO check
/// and by inbound webhook handlers. Mirrors aivory_mail_core::routing's
/// exact-match / catch-all logic, but against live tables instead of an
/// in-memory snapshot — two indexed lookups, no bulk load.
pub async fn resolve_recipient(state: &Arc<AppState>, to: &str) -> Result<RecipientResolution> {
    let norm = aivory_mail_core::validation::normalize_email(to);
    let Some(domain) = aivory_mail_core::validation::extract_domain(&norm) else {
        return Ok(reject("invalid recipient"));
    };

    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            if let Some(row) = sqlx::query("SELECT id, tenant_id FROM mailboxes WHERE lower(address)=$1 LIMIT 1")
                .bind(&norm).fetch_optional(pool).await?
            {
                return Ok(RecipientResolution {
                    accept: true,
                    mailbox_id: Some(row.get::<Uuid, _>("id")),
                    tenant_id: Some(row.get::<Uuid, _>("tenant_id")),
                    reason: "mailbox match".into(),
                });
            }
            if let Some(row) = sqlx::query(
                "SELECT m.id, m.tenant_id FROM mailboxes m JOIN domains d ON m.domain_id = d.id \
                 WHERE lower(d.domain) = $1 AND m.is_catch_all = true LIMIT 1",
            )
            .bind(&domain).fetch_optional(pool).await?
            {
                return Ok(RecipientResolution {
                    accept: true,
                    mailbox_id: Some(row.get::<Uuid, _>("id")),
                    tenant_id: Some(row.get::<Uuid, _>("tenant_id")),
                    reason: "catch-all".into(),
                });
            }
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            if let Some(row) = sqlx::query("SELECT id, tenant_id FROM mailboxes WHERE lower(address)=? LIMIT 1")
                .bind(&norm).fetch_optional(pool).await?
            {
                let id: String = row.get("id");
                let tid: String = row.get("tenant_id");
                return Ok(RecipientResolution {
                    accept: true,
                    mailbox_id: Uuid::parse_str(&id).ok(),
                    tenant_id: Uuid::parse_str(&tid).ok(),
                    reason: "mailbox match".into(),
                });
            }
            if let Some(row) = sqlx::query(
                "SELECT m.id, m.tenant_id FROM mailboxes m JOIN domains d ON m.domain_id = d.id \
                 WHERE lower(d.domain) = ? AND m.is_catch_all = 1 LIMIT 1",
            )
            .bind(&domain).fetch_optional(pool).await?
            {
                let id: String = row.get("id");
                let tid: String = row.get("tenant_id");
                return Ok(RecipientResolution {
                    accept: true,
                    mailbox_id: Uuid::parse_str(&id).ok(),
                    tenant_id: Uuid::parse_str(&tid).ok(),
                    reason: "catch-all".into(),
                });
            }
        }
    }

    Ok(reject("no mailbox found"))
}
