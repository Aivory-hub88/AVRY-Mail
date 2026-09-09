//! Reversible IMAP credential vault used only for explicitly authorized
//! administrative recovery. The Dovecot passdb continues to receive a
//! separate one-way hash in `password_hash_dovecot`.

use aes_gcm::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng, Payload},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use uuid::Uuid;

const VERSION: &str = "v1";
const NONCE_LEN: usize = 12;
const AAD_PREFIX: &[u8] = b"aivory-mail:imap-password:";

#[derive(Debug)]
pub enum VaultError {
    InvalidCiphertext,
    Encrypt,
    Decrypt,
}

fn aad(mailbox_id: Uuid) -> Vec<u8> {
    format!(
        "{}{}:{}",
        String::from_utf8_lossy(AAD_PREFIX),
        VERSION,
        mailbox_id
    )
    .into_bytes()
}

/// Encrypt a credential with a fresh 96-bit nonce. The mailbox UUID is
/// authenticated additional data, so a ciphertext copied to another row
/// cannot be decrypted there. The serialized value is `v1:<base64url>` where
/// the payload is `nonce || ciphertext || gcm_tag`.
pub fn encrypt(key: &[u8; 32], mailbox_id: Uuid, password: &str) -> Result<String, VaultError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| VaultError::Encrypt)?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: password.as_bytes(),
                aad: &aad(mailbox_id),
            },
        )
        .map_err(|_| VaultError::Encrypt)?;

    let mut serialized = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    serialized.extend_from_slice(&nonce_bytes);
    serialized.extend_from_slice(&ciphertext);
    Ok(format!("{VERSION}:{}", URL_SAFE_NO_PAD.encode(serialized)))
}

/// Decrypt a versioned credential value. Callers deliberately receive no
/// underlying cryptographic detail so this cannot become a decrypt oracle.
pub fn decrypt(key: &[u8; 32], mailbox_id: Uuid, encrypted: &str) -> Result<String, VaultError> {
    let encoded = encrypted
        .strip_prefix(&format!("{VERSION}:"))
        .ok_or(VaultError::InvalidCiphertext)?;
    let serialized = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| VaultError::InvalidCiphertext)?;
    if serialized.len() <= NONCE_LEN {
        return Err(VaultError::InvalidCiphertext);
    }
    let (nonce_bytes, ciphertext) = serialized.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| VaultError::Decrypt)?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(nonce_bytes),
            Payload {
                msg: ciphertext,
                aad: &aad(mailbox_id),
            },
        )
        .map_err(|_| VaultError::Decrypt)?;
    String::from_utf8(plaintext).map_err(|_| VaultError::Decrypt)
}
