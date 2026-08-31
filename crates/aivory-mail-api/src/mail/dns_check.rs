use aivory_mail_core::dns::{apply_found_mx, apply_found_txt, DnsRecord};
use hickory_resolver::proto::rr::{RData, RecordType};
use hickory_resolver::Resolver;

fn fqdn(name: &str) -> String {
    if name.ends_with('.') { name.to_string() } else { format!("{}.", name) }
}

async fn resolve_txt(name: &str) -> Vec<String> {
    let Ok(builder) = Resolver::builder_tokio() else { return vec![] };
    let Ok(resolver) = builder.build() else { return vec![] };
    let Ok(lookup) = resolver.lookup(fqdn(name), RecordType::TXT).await else { return vec![] };
    lookup
        .answers()
        .iter()
        .filter_map(|r| match &r.data {
            RData::TXT(txt) => Some(txt.to_string()),
            _ => None,
        })
        .collect()
}

async fn resolve_mx(name: &str) -> Vec<String> {
    let Ok(builder) = Resolver::builder_tokio() else { return vec![] };
    let Ok(resolver) = builder.build() else { return vec![] };
    let Ok(lookup) = resolver.lookup(fqdn(name), RecordType::MX).await else { return vec![] };
    lookup
        .answers()
        .iter()
        .filter_map(|r| match &r.data {
            RData::MX(mx) => Some(mx.exchange.to_string()),
            _ => None,
        })
        .collect()
}

/// Query live public DNS and fill in `status`/`found_values` for each expected record.
pub async fn check_records(mut expected: Vec<DnsRecord>) -> Vec<DnsRecord> {
    for record in expected.iter_mut() {
        if record.record_type == "MX" {
            let found = resolve_mx(&record.host).await;
            apply_found_mx(record, found);
        } else {
            let found = resolve_txt(&record.host).await;
            apply_found_txt(record, found);
        }
    }
    expected
}

/// Just the verification TXT lookup — used by POST /v1/domains/:id/verify.
pub async fn verify_ownership(domain: &str, expected_token_value: &str) -> bool {
    let found = resolve_txt(&format!("_aivory-verify.{}", domain)).await;
    found.iter().any(|v| v.trim().trim_matches('"') == expected_token_value.trim())
}
