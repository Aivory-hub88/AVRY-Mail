use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum DnsRecordStatus {
    Missing,
    Correct,
    Mismatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsRecord {
    pub record_type: String, // "MX" | "TXT"
    pub host: String,        // fully-qualified name to query
    pub purpose: String,     // "verification" | "mx" | "spf" | "dkim" | "dmarc"
    pub expected_value: String,
    pub priority: Option<u16>, // MX preference
    pub status: DnsRecordStatus,
    pub found_values: Vec<String>,
}

impl DnsRecord {
    fn new(record_type: &str, host: String, purpose: &str, expected_value: String, priority: Option<u16>) -> Self {
        Self {
            record_type: record_type.into(),
            host,
            purpose: purpose.into(),
            expected_value,
            priority,
            status: DnsRecordStatus::Missing,
            found_values: vec![],
        }
    }
}

pub struct DnsRecordInput<'a> {
    pub domain: &'a str,
    pub dkim_selector: &'a str,
    /// base64 of the DKIM public key SubjectPublicKeyInfo DER (no PEM headers, no whitespace)
    pub dkim_public_key_b64: &'a str,
    pub verification_token: &'a str,
    /// hostname mail is received on, e.g. "mail.aivory.id"
    pub mx_host: &'a str,
    /// hostname referenced by the SPF `include:` mechanism, e.g. "_spf.aivory.id"
    pub spf_include_host: &'a str,
    pub dmarc_report_address: &'a str,
}

/// The DNS records a customer must add for a domain to fully work (ownership
/// verification + inbound MX + outbound authentication). Pure/offline — no
/// network access. Live status is filled in separately against real DNS.
pub fn expected_records(input: &DnsRecordInput) -> Vec<DnsRecord> {
    vec![
        DnsRecord::new(
            "TXT",
            format!("_aivory-verify.{}", input.domain),
            "verification",
            format!("aivory-site-verification={}", input.verification_token),
            None,
        ),
        DnsRecord::new(
            "MX",
            input.domain.to_string(),
            "mx",
            format!("{}.", input.mx_host.trim_end_matches('.')),
            Some(10),
        ),
        DnsRecord::new(
            "TXT",
            input.domain.to_string(),
            "spf",
            format!("v=spf1 include:{} ~all", input.spf_include_host),
            None,
        ),
        DnsRecord::new(
            "TXT",
            format!("{}._domainkey.{}", input.dkim_selector, input.domain),
            "dkim",
            format!("v=DKIM1; k=rsa; p={}", input.dkim_public_key_b64),
            None,
        ),
        DnsRecord::new(
            "TXT",
            format!("_dmarc.{}", input.domain),
            "dmarc",
            format!("v=DMARC1; p=quarantine; rua=mailto:{}", input.dmarc_report_address),
            None,
        ),
    ]
}

fn normalize_txt(v: &str) -> String {
    v.trim().trim_matches('"').to_string()
}

/// Compare live-resolved TXT/MX values against one expected record, in place.
pub fn apply_found_txt(record: &mut DnsRecord, found: Vec<String>) {
    let expected = normalize_txt(&record.expected_value);
    record.status = if found.is_empty() {
        DnsRecordStatus::Missing
    } else if found.iter().any(|v| normalize_txt(v) == expected) {
        DnsRecordStatus::Correct
    } else {
        DnsRecordStatus::Mismatch
    };
    record.found_values = found;
}

pub fn apply_found_mx(record: &mut DnsRecord, found: Vec<String>) {
    let expected = record.expected_value.trim_end_matches('.').to_lowercase();
    record.status = if found.is_empty() {
        DnsRecordStatus::Missing
    } else if found.iter().any(|v| v.trim_end_matches('.').to_lowercase() == expected) {
        DnsRecordStatus::Correct
    } else {
        DnsRecordStatus::Mismatch
    };
    record.found_values = found;
}
