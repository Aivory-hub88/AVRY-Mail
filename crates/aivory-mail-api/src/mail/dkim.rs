use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use mail_auth::common::crypto::{RsaKey, Sha256};
use mail_auth::common::headers::HeaderWriter;
use mail_auth::dkim::DkimSigner;
use rsa::pkcs1::{EncodeRsaPrivateKey, LineEnding};
use rsa::pkcs8::EncodePublicKey;
use rsa::{RsaPrivateKey, RsaPublicKey};

/// One-time RSA-2048 keypair generation for a newly added domain.
/// Returns (private_key_pkcs1_pem, public_key_der_base64_for_dns).
pub fn generate_keypair() -> Result<(String, String)> {
    let mut rng = rand::thread_rng();
    let private_key = RsaPrivateKey::new(&mut rng, 2048).context("rsa keygen failed")?;
    let public_key = RsaPublicKey::from(&private_key);

    let private_pem = private_key
        .to_pkcs1_pem(LineEnding::LF)
        .context("encode private key pem")?
        .to_string();
    let public_der = public_key.to_public_key_der().context("encode public key der")?;
    let public_b64 = B64.encode(public_der.as_bytes());

    Ok((private_pem, public_b64))
}

/// Sign a raw RFC5322 message and return the `DKIM-Signature` header line
/// (including trailing CRLF) to prepend to the message before sending.
pub fn sign(private_key_pem: &str, selector: &str, domain: &str, raw_message: &[u8]) -> Result<String> {
    let key = RsaKey::<Sha256>::from_rsa_pem(private_key_pem).map_err(|e| anyhow::anyhow!("dkim key parse: {e}"))?;
    let signature = DkimSigner::from_key(key)
        .domain(domain)
        .selector(selector)
        .headers(["From", "To", "Subject", "Date", "Message-ID"])
        .sign(raw_message)
        .map_err(|e| anyhow::anyhow!("dkim sign failed: {e}"))?;
    Ok(signature.to_header())
}
