//! Required secrets, fail-closed in every build.
//!
//! JWT_SECRET and INTERNAL_TOKEN used to fall back to well-known dev values
//! unless RUST_ENV/ENV/NODE_ENV said "production" — and the prod compose sets
//! none of those on avry-mail or avry-mail-smtp, so that guard never fired.
//! Compose also passes `JWT_SECRET=${JWT_SECRET}`, which turns an unset .env
//! entry into an empty string that the old code accepted as the HMAC key.
//! Now a missing, blank or placeholder value refuses to start, everywhere.

/// Values that are public in this repo and must never authenticate anything.
const PLACEHOLDERS: &[&str] = &[
    "aivory-mail-dev-secret-change-me",
    "aivory-internal-dev",
    "change-me-in-development",
    "replace-with-a-long-random-password",
    "change-me",
    "changeme",
    "secret",
];

/// The secret as given, or why it can't be used. Returned unchanged (not
/// trimmed) so a value shared with another service still matches byte for byte.
pub fn validate_secret(name: &str, value: Option<String>) -> Result<String, String> {
    let value = value.unwrap_or_default();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{name} is not set; refusing to start"));
    }
    if PLACEHOLDERS.iter().any(|p| trimmed.eq_ignore_ascii_case(p)) {
        return Err(format!("{name} is a placeholder value; refusing to start"));
    }
    Ok(value)
}

/// Read a required secret from the environment or exit the process.
pub fn require_env_secret(name: &str) -> String {
    match validate_secret(name, std::env::var(name).ok()) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("[FATAL] {msg}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::validate_secret;

    #[test]
    fn missing_or_blank_is_rejected() {
        assert!(validate_secret("JWT_SECRET", None).is_err());
        assert!(validate_secret("JWT_SECRET", Some(String::new())).is_err());
        assert!(validate_secret("JWT_SECRET", Some("   ".into())).is_err());
    }

    #[test]
    fn old_dev_defaults_are_rejected() {
        assert!(validate_secret("JWT_SECRET", Some("aivory-mail-dev-secret-change-me".into())).is_err());
        assert!(validate_secret("INTERNAL_TOKEN", Some("aivory-internal-dev".into())).is_err());
        assert!(validate_secret("INTERNAL_TOKEN", Some("ChangeMe".into())).is_err());
    }

    #[test]
    fn real_value_is_returned_unchanged() {
        assert_eq!(validate_secret("JWT_SECRET", Some("s3cr3t ".into())).unwrap(), "s3cr3t ");
    }

    #[test]
    fn error_names_the_variable_not_the_value() {
        let err = validate_secret("INTERNAL_TOKEN", Some("aivory-internal-dev".into())).unwrap_err();
        assert!(err.contains("INTERNAL_TOKEN"));
        assert!(!err.contains("aivory-internal-dev"));
    }
}
