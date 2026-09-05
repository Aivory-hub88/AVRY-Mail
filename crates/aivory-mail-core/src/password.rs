//! Per-mailbox password hashing. Salted SHA-256 (PBKDF2-style stretching),
//! stored as `sha256$<iterations>$<salt_hex>$<hash_hex>` so the format can be
//! upgraded later (e.g. to argon2) without breaking already-hashed rows.
use rand::RngCore;
use sha2::{Digest, Sha256};

const ITERATIONS: u32 = 100_000;

pub fn hash_password(password: &str) -> String {
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let hash = stretch(password.as_bytes(), &salt, ITERATIONS);
    format!("sha256${}${}${}", ITERATIONS, hex::encode(salt), hex::encode(hash))
}

pub fn verify_password(password: &str, stored: &str) -> bool {
    let parts: Vec<&str> = stored.split('$').collect();
    if parts.len() != 4 || parts[0] != "sha256" {
        return false;
    }
    let Ok(iterations) = parts[1].parse::<u32>() else { return false };
    let Ok(salt) = hex::decode(parts[2]) else { return false };
    let Ok(expected) = hex::decode(parts[3]) else { return false };
    let actual = stretch(password.as_bytes(), &salt, iterations);
    actual == expected
}

fn stretch(password: &[u8], salt: &[u8], iterations: u32) -> Vec<u8> {
    let mut state = Sha256::digest([salt, password].concat()).to_vec();
    for _ in 1..iterations {
        state = Sha256::digest(&state).to_vec();
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let h = hash_password("hunter2");
        assert!(verify_password("hunter2", &h));
        assert!(!verify_password("wrong", &h));
    }
}
