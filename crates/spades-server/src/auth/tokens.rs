//! Single-use email tokens (verify-email, password-reset).

use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const PURPOSE_VERIFY_EMAIL: &str = "verify_email";
pub const PURPOSE_PASSWORD_RESET: &str = "password_reset";

pub fn generate_token() -> String {
    let mut buf = [0u8; 32];
    rand::rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[derive(Debug, Clone)]
pub struct ConsumedToken {
    pub user_id: Uuid,
    pub purpose: String,
}
