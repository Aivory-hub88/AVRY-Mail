use crate::api::AppState;
use anyhow::Result;
use serde_json::Value;
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

/// DB-backed recipient resolution — 3-phase Mailflare parity:
/// 1) domain scope reject (block sender even if mailbox exists)
/// 2) exact mailbox → alias → use_all_domains → catch-all
pub async fn resolve_recipient(state: &Arc<AppState>, to: &str) -> Result<RecipientResolution> {
    let norm = aivory_mail_core::validation::normalize_email(to);
    let Some(domain) = aivory_mail_core::validation::extract_domain(&norm) else {
        return Ok(reject("invalid recipient"));
    };
    let Some(local) = norm.split('@').next() else { return Ok(reject("invalid recipient")); };
    let local_lc = local.to_lowercase();

    // Phase 1: domain scope reject (Mailflare: domain reject has highest priority)
    // For inbound, `from` is not known at RCPT TO time, so we check only recipient-based domain rules here.
    // Sender-based reject (from:"*") is handled in inbound.rs after parsing.
    // Here we handle recipient domain catch-all forward/store if needed, but for now just mailbox phase.

    match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            // Exact mailbox
            if let Some(row) = sqlx::query("SELECT id, tenant_id FROM mailboxes WHERE lower(address)=$1 LIMIT 1")
                .bind(&norm).fetch_optional(pool).await?
            {
                return Ok(RecipientResolution {
                    accept: true,
                    mailbox_id: Some(row.try_get::<Uuid,_>("id").map(|u| u).unwrap_or_else(|_| Uuid::parse_str(&row.try_get::<String,_>("id").unwrap_or_default()).unwrap_or(Uuid::nil()))),
                    tenant_id: Some(row.try_get::<Uuid,_>("tenant_id").map(|u| u).unwrap_or_else(|_| Uuid::parse_str(&row.try_get::<String,_>("tenant_id").unwrap_or_default()).unwrap_or(Uuid::nil()))),
                    reason: "mailbox match".into(),
                });
            }
            // Alias lookup (mailbox_aliases local_part)
            if let Some(row) = sqlx::query("SELECT ma.mailbox_id, m.tenant_id FROM mailbox_aliases ma JOIN mailboxes m ON ma.mailbox_id=m.id JOIN domains d ON ma.domain_id=d.id WHERE lower(d.domain)=$1 AND lower(ma.local_part)=$2 LIMIT 1")
                .bind(&domain).bind(&local_lc).fetch_optional(pool).await? {
                return Ok(RecipientResolution {
                    accept: true,
                    mailbox_id: Some(row.try_get::<Uuid,_>("mailbox_id").map(|u| u).unwrap_or_else(|_| Uuid::parse_str(&row.try_get::<String,_>("mailbox_id").unwrap_or_default()).unwrap_or(Uuid::nil()))),
                    tenant_id: Some(row.try_get::<Uuid,_>("tenant_id").map(|u| u).unwrap_or_else(|_| Uuid::parse_str(&row.try_get::<String,_>("tenant_id").unwrap_or_default()).unwrap_or(Uuid::nil()))),
                    reason: "alias".into(),
                });
            }
            // use_all_domains: mailbox with use_all_domains = true can receive any domain
            if let Some(row) = sqlx::query("SELECT id, tenant_id FROM mailboxes WHERE use_all_domains=true AND lower(address) LIKE $1 LIMIT 1")
                .bind(format!("{}@%", local_lc)).fetch_optional(pool).await? {
                // This is a simplified check: any mailbox that has use_all_domains and local part matches
                // More precise: check if local matches any mailbox's local part where use_all_domains
                let all_rows = sqlx::query("SELECT id, tenant_id, address FROM mailboxes WHERE use_all_domains=true").fetch_all(pool).await?;
                for r in all_rows {
                    let addr: String = r.try_get::<String,_>("address").unwrap_or_default();
                    if let Some(l) = addr.split('@').next() { if l.to_lowercase() == local_lc {
                        return Ok(RecipientResolution {
                            accept: true,
                            mailbox_id: Some(r.try_get::<Uuid,_>("id").map(|u| u).unwrap_or_else(|_| Uuid::parse_str(&r.try_get::<String,_>("id").unwrap_or_default()).unwrap_or(Uuid::nil()))),
                            tenant_id: Some(r.try_get::<Uuid,_>("tenant_id").map(|u| u).unwrap_or_else(|_| Uuid::parse_str(&r.try_get::<String,_>("tenant_id").unwrap_or_default()).unwrap_or(Uuid::nil()))),
                            reason: "use_all_domains".into(),
                        });
                    }}
                }
            }
            // Catch-all
            if let Some(row) = sqlx::query(
                "SELECT m.id, m.tenant_id FROM mailboxes m JOIN domains d ON m.domain_id = d.id \
                 WHERE lower(d.domain) = $1 AND m.is_catch_all = true LIMIT 1",
            )
            .bind(&domain).fetch_optional(pool).await?
            {
                return Ok(RecipientResolution {
                    accept: true,
                    mailbox_id: Some(row.try_get::<Uuid,_>("id").map(|u| u).unwrap_or_else(|_| Uuid::parse_str(&row.try_get::<String,_>("id").unwrap_or_default()).unwrap_or(Uuid::nil()))),
                    tenant_id: Some(row.try_get::<Uuid,_>("tenant_id").map(|u| u).unwrap_or_else(|_| Uuid::parse_str(&row.try_get::<String,_>("tenant_id").unwrap_or_default()).unwrap_or(Uuid::nil()))),
                    reason: "catch-all".into(),
                });
            }
            // Domain scope forward/store catch-all with pattern "*" (Mailflare)
            if let Some(row) = sqlx::query("SELECT criteria_json, action_json FROM mail_filters WHERE tenant_id='default' AND scope='domain' AND enabled=true ORDER BY priority ASC, created_at ASC").fetch_all(pool).await.ok().and_then(|rows| {
                let mut found = None;
                for r in rows {
                    let crit_s: String = r.try_get::<String,_>("criteria_json").unwrap_or_default();
                    let act_s: String = r.try_get::<String,_>("action_json").unwrap_or_default();
                    let crit: serde_json::Value = serde_json::from_str(&crit_s).unwrap_or_default();
                    let act: serde_json::Value = serde_json::from_str(&act_s).unwrap_or_default();
                    // Check if this is a catch-all forward/store rule (from:"*" or pattern "*")
                    if let Some(obj) = crit.as_object() {
                        if obj.values().any(|v| v.as_str() == Some("*")) {
                            // This is a domain catch-all, check if it matches domain
                            if let Some(pat) = crit.get("domain").and_then(|v| v.as_str()) {
                                if pat == "*" || pat.to_lowercase() == domain.to_lowercase() {
                                    found = Some((crit_s, act_s));
                                    break;
                                }
                            } else if crit.get("from").and_then(|v| v.as_str()) == Some("*") {
                                found = Some((crit_s, act_s));
                                break;
                            }
                        }
                    }
                }
                found
            }) {
                // For now, if we found a domain catch-all, we still need a mailbox to deliver to — use catch-all mailbox if exists, otherwise accept as domain forward
                let act_val: Value = serde_json::from_str(&row.1).unwrap_or_default();
                if let Some(forward) = act_val.get("forward").and_then(|v| v.as_str()) {
                    // This would be a forward, but we still need to accept
                    return Ok(RecipientResolution { accept: true, mailbox_id: None, tenant_id: None, reason: format!("domain forward to {}", forward) });
                }
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
            if let Some(row) = sqlx::query("SELECT ma.mailbox_id, m.tenant_id FROM mailbox_aliases ma JOIN mailboxes m ON ma.mailbox_id=m.id JOIN domains d ON ma.domain_id=d.id WHERE lower(d.domain)=? AND lower(ma.local_part)=? LIMIT 1")
                .bind(&domain).bind(&local_lc).fetch_optional(pool).await? {
                let mid: String = row.get("mailbox_id");
                let tid: String = row.get("tenant_id");
                return Ok(RecipientResolution {
                    accept: true,
                    mailbox_id: Uuid::parse_str(&mid).ok(),
                    tenant_id: Uuid::parse_str(&tid).ok(),
                    reason: "alias".into(),
                });
            }
            // use_all_domains
            let all_rows = sqlx::query("SELECT id, tenant_id, address FROM mailboxes WHERE use_all_domains=1").fetch_all(pool).await.unwrap_or_default();
            for r in all_rows {
                let addr: String = r.get("address");
                if let Some(l) = addr.split('@').next() { if l.to_lowercase() == local_lc {
                    let id: String = r.get("id");
                    let tid: String = r.get("tenant_id");
                    return Ok(RecipientResolution {
                        accept: true,
                        mailbox_id: Uuid::parse_str(&id).ok(),
                        tenant_id: Uuid::parse_str(&tid).ok(),
                        reason: "use_all_domains".into(),
                    });
                }}
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
