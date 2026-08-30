use anyhow::{bail, Result};
use regex::Regex;
use std::sync::OnceLock;

fn email_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}$").unwrap())
}

fn domain_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?:[a-zA-Z0-9](?:[a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]{2,}$").unwrap())
}

pub fn validate_email(addr: &str) -> Result<()> {
    if !email_regex().is_match(addr) { bail!("invalid email address: {}", addr); }
    Ok(())
}

pub fn validate_domain(domain: &str) -> Result<()> {
    let d = domain.trim().to_lowercase();
    if d.len() > 253 { bail!("domain too long"); }
    if !domain_regex().is_match(&d) { bail!("invalid domain: {}", domain); }
    Ok(())
}

pub fn normalize_email(addr: &str) -> String { addr.trim().to_lowercase() }
pub fn normalize_domain(d: &str) -> String { d.trim().to_lowercase() }

pub fn extract_domain(email: &str) -> Option<String> {
    email.split('@').nth(1).map(|d| d.to_lowercase())
}
