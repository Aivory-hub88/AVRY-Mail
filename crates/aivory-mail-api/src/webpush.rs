//! Web Push: new-mail notifications that reach a browser even when no Aivory
//! Mail tab is open (the in-page pop-ups only work while a tab is alive).
//!
//! Implements the two standards directly on RustCrypto primitives already in
//! the tree (p256, hkdf, sha2, aes-gcm) instead of pulling in a push crate
//! with its own HTTP stack:
//!   - RFC 8291 message encryption ("aes128gcm" content coding, RFC 8188);
//!   - RFC 8292 VAPID, an ES256 JWT that identifies this server to the
//!     browser vendor's push service.
//!
//! Push is optional per deployment: without WEB_PUSH_VAPID_PRIVATE_KEY the
//! feature reports itself disabled and nothing is sent.

use aes_gcm::{aead::Aead, Aes128Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64URL, Engine as _};
use hkdf::Hkdf;
use p256::{
    ecdh::diffie_hellman,
    ecdsa::{signature::Signer, Signature, SigningKey},
    elliptic_curve::sec1::ToEncodedPoint,
    PublicKey, SecretKey,
};
use sha2::Sha256;

/// Record size advertised in the aes128gcm header. One record is enough:
/// our payloads are a few hundred bytes.
const RECORD_SIZE: u32 = 4096;
/// Push services cap the encrypted body (4096 for most); keep headroom.
pub const MAX_PAYLOAD_BYTES: usize = 3000;

#[derive(Clone)]
pub struct Vapid {
    signing_key: SigningKey,
    /// Uncompressed P-256 public key, base64url: what the browser's
    /// `applicationServerKey` needs and what goes in the `k=` parameter.
    pub public_key_b64: String,
    /// `mailto:` or `https:` contact the push service can reach us at.
    pub subject: String,
}

impl std::fmt::Debug for Vapid {
    // Never print the private key.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vapid").field("public_key_b64", &self.public_key_b64).field("subject", &self.subject).finish()
    }
}

impl Vapid {
    /// `private_key_b64` is the raw 32-byte P-256 scalar, base64url (the
    /// format `web-push generate-vapid-keys` and most tooling emit).
    pub fn from_private_key(private_key_b64: &str, subject: &str) -> Result<Self, String> {
        let raw = B64URL
            .decode(private_key_b64.trim().trim_end_matches('='))
            .map_err(|_| "WEB_PUSH_VAPID_PRIVATE_KEY is not base64url".to_string())?;
        let secret = SecretKey::from_slice(&raw).map_err(|_| "WEB_PUSH_VAPID_PRIVATE_KEY is not a P-256 key".to_string())?;
        let subject = subject.trim();
        if !(subject.starts_with("mailto:") || subject.starts_with("https://")) {
            return Err("WEB_PUSH_SUBJECT must start with mailto: or https://".into());
        }
        let public = secret.public_key().to_encoded_point(false);
        Ok(Self {
            signing_key: SigningKey::from(secret),
            public_key_b64: B64URL.encode(public.as_bytes()),
            subject: subject.to_string(),
        })
    }

    /// Read from env. `Ok(None)` = not configured (push disabled);
    /// `Err` = configured but broken (the caller refuses to start).
    pub fn from_env() -> Result<Option<Self>, String> {
        let key = std::env::var("WEB_PUSH_VAPID_PRIVATE_KEY").unwrap_or_default();
        if key.trim().is_empty() {
            return Ok(None);
        }
        let subject = std::env::var("WEB_PUSH_SUBJECT").unwrap_or_default();
        Self::from_private_key(&key, &subject).map(Some)
    }

    /// `Authorization` header value for one push request (RFC 8292 §3).
    pub fn authorization(&self, endpoint: &str, now_unix: u64) -> Result<String, String> {
        let url = reqwest::Url::parse(endpoint).map_err(|_| "bad push endpoint".to_string())?;
        let audience = url.origin().ascii_serialization();
        let header = B64URL.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = serde_json::json!({
            "aud": audience,
            // Push services reject exp more than 24h out.
            "exp": now_unix + 12 * 3600,
            "sub": self.subject,
        });
        let claims = B64URL.encode(claims.to_string().as_bytes());
        let signing_input = format!("{header}.{claims}");
        let sig: Signature = self.signing_key.sign(signing_input.as_bytes());
        let jwt = format!("{signing_input}.{}", B64URL.encode(sig.to_bytes()));
        Ok(format!("vapid t={jwt}, k={}", self.public_key_b64))
    }
}

/// A browser's subscription, as `PushSubscription.toJSON()` reports it.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Subscription {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

fn hkdf(salt: &[u8], ikm: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, &mut out)
        .expect("hkdf output length is always valid here");
    out
}

/// RFC 8291 §3 + RFC 8188: encrypt `payload` for one subscription, with the
/// given one-off server key and salt (random in production, fixed in tests).
pub fn encrypt_with(
    payload: &[u8],
    ua_public_b64: &str,
    auth_secret_b64: &str,
    as_secret: &SecretKey,
    salt: &[u8; 16],
) -> Result<Vec<u8>, String> {
    let ua_public_raw = B64URL.decode(ua_public_b64.trim_end_matches('=')).map_err(|_| "bad p256dh".to_string())?;
    let ua_public = PublicKey::from_sec1_bytes(&ua_public_raw).map_err(|_| "bad p256dh".to_string())?;
    let auth_secret = B64URL.decode(auth_secret_b64.trim_end_matches('=')).map_err(|_| "bad auth".to_string())?;
    if auth_secret.len() != 16 {
        return Err("bad auth".into());
    }
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err("payload too large".into());
    }

    let as_public = as_secret.public_key().to_encoded_point(false);
    let shared = diffie_hellman(as_secret.to_nonzero_scalar(), ua_public.as_affine());

    // IKM = HKDF(auth_secret, ecdh_secret, "WebPush: info\0" || ua_public || as_public)
    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(ua_public.to_encoded_point(false).as_bytes());
    key_info.extend_from_slice(as_public.as_bytes());
    let ikm = hkdf(&auth_secret, shared.raw_secret_bytes().as_slice(), &key_info, 32);

    let cek = hkdf(salt, &ikm, b"Content-Encoding: aes128gcm\0", 16);
    let nonce = hkdf(salt, &ikm, b"Content-Encoding: nonce\0", 12);

    // Single, final record: payload followed by the 0x02 delimiter.
    let mut plaintext = payload.to_vec();
    plaintext.push(0x02);
    let cipher = Aes128Gcm::new_from_slice(&cek).map_err(|_| "cek".to_string())?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_slice())
        .map_err(|_| "encrypt".to_string())?;

    // Header: salt(16) || rs(4) || idlen(1) || keyid(as_public, 65)
    let mut body = Vec::with_capacity(16 + 4 + 1 + 65 + ciphertext.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(as_public.as_bytes().len() as u8);
    body.extend_from_slice(as_public.as_bytes());
    body.extend_from_slice(&ciphertext);
    Ok(body)
}

/// Encrypt with a fresh random server key and salt (what a real send uses).
pub fn encrypt(payload: &[u8], sub: &Subscription) -> Result<Vec<u8>, String> {
    use rand::RngCore;
    let as_secret = SecretKey::random(&mut rand::rngs::OsRng);
    let mut salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    encrypt_with(payload, &sub.p256dh, &sub.auth, &as_secret, &salt)
}

/// What happened to one push, from the subscription's point of view.
#[derive(Debug, PartialEq, Eq)]
pub enum SendOutcome {
    Delivered,
    /// 404/410: the browser unsubscribed or the subscription expired.
    /// Delete it; retrying can never succeed.
    Gone,
    /// Anything else (429, 5xx, network): keep it, try again next mail.
    Failed(String),
}

pub fn classify_status(status: u16) -> SendOutcome {
    match status {
        200..=299 => SendOutcome::Delivered,
        404 | 410 => SendOutcome::Gone,
        other => SendOutcome::Failed(format!("push service returned {other}")),
    }
}

pub async fn send(client: &reqwest::Client, vapid: &Vapid, sub: &Subscription, payload: &serde_json::Value) -> SendOutcome {
    let body = match encrypt(payload.to_string().as_bytes(), sub) {
        Ok(b) => b,
        Err(e) => return SendOutcome::Failed(e),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let auth = match vapid.authorization(&sub.endpoint, now) {
        Ok(a) => a,
        // An endpoint we can't even parse is as good as gone.
        Err(_) => return SendOutcome::Gone,
    };
    let res = client
        .post(&sub.endpoint)
        .header("Authorization", auth)
        .header("Content-Encoding", "aes128gcm")
        .header("Content-Type", "application/octet-stream")
        // Kept a day by the push service if the device is offline.
        .header("TTL", "86400")
        .header("Urgency", "high")
        .body(body)
        .send()
        .await;
    match res {
        Ok(r) => classify_status(r.status().as_u16()),
        Err(e) => SendOutcome::Failed(e.to_string()),
    }
}

/// Only push-service hosts the major browsers use. A subscription endpoint
/// is client-supplied; without this the server could be made to POST to any
/// URL (SSRF).
pub fn is_allowed_endpoint(endpoint: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(endpoint) else { return false };
    if url.scheme() != "https" {
        return false;
    }
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    const SUFFIXES: &[&str] = &[
        "fcm.googleapis.com",           // Chrome, Edge (FCM), Opera, Brave
        "push.services.mozilla.com",    // Firefox
        "notify.windows.com",           // Edge (WNS)
        "push.apple.com",               // Safari (web.push.apple.com)
    ];
    SUFFIXES.iter().any(|s| host == *s || host.ends_with(&format!(".{s}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decrypt as the browser would (RFC 8291 from the user agent's side),
    /// written independently of `encrypt_with` so the round trip checks the
    /// derivation rather than repeating it.
    fn ua_decrypt(body: &[u8], ua_secret: &SecretKey, auth_secret: &[u8]) -> Vec<u8> {
        let salt = &body[0..16];
        let rs = u32::from_be_bytes(body[16..20].try_into().unwrap());
        assert_eq!(rs, RECORD_SIZE);
        let idlen = body[20] as usize;
        let as_public = PublicKey::from_sec1_bytes(&body[21..21 + idlen]).unwrap();
        let ciphertext = &body[21 + idlen..];
        let shared = diffie_hellman(ua_secret.to_nonzero_scalar(), as_public.as_affine());
        let mut info = b"WebPush: info\0".to_vec();
        info.extend_from_slice(ua_secret.public_key().to_encoded_point(false).as_bytes());
        info.extend_from_slice(as_public.to_encoded_point(false).as_bytes());
        let ikm = hkdf(auth_secret, shared.raw_secret_bytes().as_slice(), &info, 32);
        let cek = hkdf(salt, &ikm, b"Content-Encoding: aes128gcm\0", 16);
        let nonce = hkdf(salt, &ikm, b"Content-Encoding: nonce\0", 12);
        let mut plain = Aes128Gcm::new_from_slice(&cek).unwrap().decrypt(Nonce::from_slice(&nonce), ciphertext).unwrap();
        assert_eq!(plain.pop(), Some(0x02), "last-record delimiter");
        plain
    }

    fn b64(s: &str) -> Vec<u8> {
        B64URL.decode(s).unwrap()
    }

    /// RFC 8291 Appendix A keys and salt.
    const RFC_AS_PRIVATE: &str = "yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw";
    const RFC_UA_PRIVATE: &str = "q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94";
    const RFC_UA_PUBLIC: &str = "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
    const RFC_SALT: &str = "DGv6ra1nlYgDCS1FRnbzlw";
    const RFC_AUTH: &str = "BTBZMqHH6r4Tts7J_aSIgg";
    const RFC_PLAINTEXT: &str = "When I grow up, I want to be a watermelon";

    #[test]
    fn rfc8291_keys_round_trip() {
        let as_secret = SecretKey::from_slice(&b64(RFC_AS_PRIVATE)).unwrap();
        let ua_secret = SecretKey::from_slice(&b64(RFC_UA_PRIVATE)).unwrap();
        assert_eq!(B64URL.encode(ua_secret.public_key().to_encoded_point(false).as_bytes()), RFC_UA_PUBLIC);
        let salt: [u8; 16] = b64(RFC_SALT).try_into().unwrap();
        let body = encrypt_with(RFC_PLAINTEXT.as_bytes(), RFC_UA_PUBLIC, RFC_AUTH, &as_secret, &salt).unwrap();
        assert_eq!(&body[..16], &salt);
        assert_eq!(ua_decrypt(&body, &ua_secret, &b64(RFC_AUTH)), RFC_PLAINTEXT.as_bytes());
    }

    /// RFC 8291 Appendix A: the exact encrypted message the spec publishes
    /// for these keys and salt. Matching it byte for byte proves the
    /// derivation against the standard, not just against our own decrypt.
    #[test]
    fn rfc8291_appendix_a_exact_ciphertext() {
        let as_secret = SecretKey::from_slice(&b64(RFC_AS_PRIVATE)).unwrap();
        let salt: [u8; 16] = b64(RFC_SALT).try_into().unwrap();
        let body = encrypt_with(RFC_PLAINTEXT.as_bytes(), RFC_UA_PUBLIC, RFC_AUTH, &as_secret, &salt).unwrap();
        assert_eq!(
            B64URL.encode(&body),
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN"
        );
    }

    #[test]
    fn random_send_encryption_round_trips() {
        let ua_secret = SecretKey::random(&mut rand::rngs::OsRng);
        let sub = Subscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/x".into(),
            p256dh: B64URL.encode(ua_secret.public_key().to_encoded_point(false).as_bytes()),
            auth: B64URL.encode([7u8; 16]),
        };
        let body = encrypt(br#"{"title":"hi"}"#, &sub).unwrap();
        assert_eq!(ua_decrypt(&body, &ua_secret, &[7u8; 16]), br#"{"title":"hi"}"#);
    }

    #[test]
    fn rejects_bad_subscription_keys_and_oversized_payloads() {
        let as_secret = SecretKey::random(&mut rand::rngs::OsRng);
        let salt = [0u8; 16];
        assert!(encrypt_with(b"x", "not-a-key", RFC_AUTH, &as_secret, &salt).is_err());
        assert!(encrypt_with(b"x", RFC_UA_PUBLIC, "c2hvcnQ", &as_secret, &salt).is_err());
        let big = vec![b'a'; MAX_PAYLOAD_BYTES + 1];
        assert!(encrypt_with(&big, RFC_UA_PUBLIC, RFC_AUTH, &as_secret, &salt).is_err());
    }

    #[test]
    fn vapid_jwt_verifies_against_the_advertised_public_key() {
        use p256::ecdsa::{signature::Verifier, VerifyingKey};
        let vapid = Vapid::from_private_key(RFC_AS_PRIVATE, "mailto:ops@aivory.uk").unwrap();
        let auth = vapid.authorization("https://fcm.googleapis.com/fcm/send/abc", 1_000).unwrap();
        let (t, k) = auth.strip_prefix("vapid t=").unwrap().split_once(", k=").unwrap();
        assert_eq!(k, vapid.public_key_b64);
        let mut parts = t.split('.');
        let (h, c, s) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
        let claims: serde_json::Value = serde_json::from_slice(&b64(c)).unwrap();
        assert_eq!(claims["aud"], "https://fcm.googleapis.com");
        assert_eq!(claims["exp"], 1_000 + 12 * 3600);
        assert_eq!(claims["sub"], "mailto:ops@aivory.uk");
        let key = VerifyingKey::from_sec1_bytes(&b64(k)).unwrap();
        let sig = Signature::from_slice(&b64(s)).unwrap();
        key.verify(format!("{h}.{c}").as_bytes(), &sig).expect("ES256 signature");
    }

    #[test]
    fn vapid_config_is_validated_and_never_printed() {
        assert!(Vapid::from_private_key("!!", "mailto:a@b.c").is_err());
        assert!(Vapid::from_private_key(RFC_AS_PRIVATE, "ops@aivory.uk").is_err());
        let v = Vapid::from_private_key(RFC_AS_PRIVATE, "mailto:a@b.c").unwrap();
        assert!(!format!("{v:?}").contains(RFC_AS_PRIVATE));
    }

    #[test]
    fn only_real_push_services_are_accepted() {
        for ok in [
            "https://fcm.googleapis.com/fcm/send/abc",
            "https://updates.push.services.mozilla.com/wpush/v2/abc",
            "https://web.push.apple.com/abc",
            "https://wns2-par02p.notify.windows.com/w/?token=abc",
        ] {
            assert!(is_allowed_endpoint(ok), "{ok}");
        }
        for bad in [
            "http://fcm.googleapis.com/fcm/send/abc",
            "https://evil.example/fcm.googleapis.com",
            "https://fcm.googleapis.com.evil.example/x",
            "https://localhost:8095/v1/admin",
            "https://169.254.169.254/latest",
            "not a url",
        ] {
            assert!(!is_allowed_endpoint(bad), "{bad}");
        }
    }

    #[test]
    fn dead_subscriptions_are_told_apart_from_transient_failures() {
        assert_eq!(classify_status(201), SendOutcome::Delivered);
        assert_eq!(classify_status(410), SendOutcome::Gone);
        assert_eq!(classify_status(404), SendOutcome::Gone);
        assert!(matches!(classify_status(429), SendOutcome::Failed(_)));
        assert!(matches!(classify_status(503), SendOutcome::Failed(_)));
    }
}
