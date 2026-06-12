//! argon2id password hashing + weak-password reject list.

use crate::auth::AuthError;
use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use std::sync::OnceLock;

const WEAK_PASSWORDS_RAW: &str = include_str!("weak_passwords.txt");

fn weak_set() -> &'static std::collections::HashSet<&'static str> {
    static SET: OnceLock<std::collections::HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        WEAK_PASSWORDS_RAW
            .lines()
            .filter(|l| !l.is_empty())
            .collect()
    })
}

fn argon2() -> Argon2<'static> {
    // `cfg!` keeps both branches compiling regardless of the feature, so the
    // production params can't bit-rot while tests run with the cheap ones.
    let params = if cfg!(feature = "insecure-fast-hash") {
        Params::new(Params::MIN_M_COST, 1, 1, None)
    } else {
        Params::new(64 * 1024, 3, 4, None)
    };
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        params.expect("valid argon2 params"),
    )
}

pub fn validate_password(password: &str) -> Result<(), AuthError> {
    if password.len() < 8 {
        return Err(AuthError::Validation(
            "password must be at least 8 characters".into(),
        ));
    }
    if password.len() > 256 {
        return Err(AuthError::Validation(
            "password must be at most 256 characters".into(),
        ));
    }
    if weak_set().contains(password.to_lowercase().as_str()) {
        return Err(AuthError::Validation("password is too common".into()));
    }
    Ok(())
}

pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AuthError::Internal(format!("password hash: {e}")))
}

pub fn verify_password(password: &str, phc: &str) -> Result<bool, AuthError> {
    let parsed =
        PasswordHash::new(phc).map_err(|e| AuthError::Internal(format!("password parse: {e}")))?;
    match argon2().verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(AuthError::Internal(format!("password verify: {e}"))),
    }
}

pub fn dummy_hash() -> &'static str {
    static H: OnceLock<String> = OnceLock::new();
    H.get_or_init(|| hash_password("placeholder-not-a-real-password").unwrap())
}

pub fn verify_against_dummy() {
    let _ = verify_password("anything", dummy_hash());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let h = hash_password("hunter2-strong").unwrap();
        assert!(verify_password("hunter2-strong", &h).unwrap());
        assert!(!verify_password("wrong-password", &h).unwrap());
    }

    #[test]
    fn validate_rejects_short() {
        assert!(validate_password("short").is_err());
    }

    #[test]
    fn validate_rejects_weak_list() {
        assert!(validate_password("password").is_err());
        assert!(validate_password("Password").is_err());
        assert!(validate_password("12345678").is_err());
    }

    #[test]
    fn validate_accepts_strong() {
        validate_password("hunter2-strong").unwrap();
        validate_password("a-reasonable-passphrase!").unwrap();
    }

    #[test]
    fn dummy_hash_can_be_verified_against() {
        verify_against_dummy();
    }
}
