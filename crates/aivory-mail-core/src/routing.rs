use crate::types::*;
use crate::validation::{extract_domain, normalize_email};
use anyhow::{bail, Result};

#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub action: RoutingAction,
    pub mailbox_id: Option<uuid::Uuid>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RoutingAction { Store, Forward(String), Reject, Discard }

/// Resolve inbound address against known mailboxes/domains.
/// In production this queries DB; here is pure logic helper.
pub fn resolve_address(
    to: &str,
    mailboxes: &[Mailbox],
    domains: &[Domain],
) -> RoutingDecision {
    let norm = normalize_email(to);
    // exact mailbox match
    if let Some(mb) = mailboxes.iter().find(|m| m.address.to_lowercase() == norm) {
        if let Some(fwd) = &mb.forward_to {
            return RoutingDecision { action: RoutingAction::Forward(fwd.clone()), mailbox_id: Some(mb.id), reason: "mailbox forward".into() };
        }
        return RoutingDecision { action: RoutingAction::Store, mailbox_id: Some(mb.id), reason: "mailbox match".into() };
    }
    // catch-all for domain
    if let Some(domain) = extract_domain(&norm) {
        if let Some(d) = domains.iter().find(|d| d.domain == domain) {
            if let Some(mb) = mailboxes.iter().find(|m| m.domain_id == d.id && m.is_catch_all) {
                return RoutingDecision { action: RoutingAction::Store, mailbox_id: Some(mb.id), reason: "catch-all".into() };
            }
        }
    }
    RoutingDecision { action: RoutingAction::Reject, mailbox_id: None, reason: "no mailbox found".into() }
}

pub fn validate_send_request(req: &SendRequest) -> Result<()> {
    crate::validation::validate_email(&req.from)?;
    if req.to.is_empty() { bail!("at least one recipient required"); }
    for t in &req.to { crate::validation::validate_email(t)?; }
    if let Some(cc) = &req.cc { for c in cc { crate::validation::validate_email(c)?; } }
    if let Some(bcc) = &req.bcc { for b in bcc { crate::validation::validate_email(b)?; } }
    if req.subject.trim().is_empty() { bail!("subject required"); }
    if req.text.is_none() && req.html.is_none() { bail!("text or html body required"); }
    // attachment limits: max 10 files, 10MB each (checked at API layer), 20MB combined
    if let Some(atts) = &req.attachments {
        if atts.len() > 10 { bail!("too many attachments (max 10)"); }
    }
    Ok(())
}
