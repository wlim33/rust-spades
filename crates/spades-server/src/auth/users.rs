//! User struct, repo (CRUD), username rules, token_version.

use crate::auth::AuthError;

/// Lowercased canonical form used for uniqueness lookups.
pub fn canonicalize_username(input: &str) -> String {
    input.to_ascii_lowercase()
}

const RESERVED: &[&str] = &[
    "me", "admin", "root", "auth", "oauth", "api",
    "users", "games", "lobbies", "challenges", "matchmaking",
    "ws", "static", "assets", "docs", "openapi", "swagger-ui",
    "player", "spades", "system", "null", "undefined",
];

pub fn validate_username(input: &str) -> Result<String, AuthError> {
    if input.len() < 2 || input.len() > 20 {
        return Err(AuthError::Validation("username must be 2-20 characters".into()));
    }
    if !input.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(AuthError::Validation("username may only contain letters, digits, underscore, hyphen".into()));
    }
    if input.starts_with('-') || input.ends_with('-') || input.contains("--") {
        return Err(AuthError::Validation("invalid hyphen placement".into()));
    }
    let canon = canonicalize_username(input);
    if RESERVED.iter().any(|r| **r == canon) {
        return Err(AuthError::Validation("username is reserved".into()));
    }
    Ok(input.to_string())
}

pub fn validate_email(input: &str) -> Result<(), AuthError> {
    let trimmed = input.trim();
    if trimmed.len() > 254 {
        return Err(AuthError::Validation("email too long".into()));
    }
    let parts: Vec<&str> = trimmed.split('@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(AuthError::Validation("invalid email syntax".into()));
    }
    if !parts[1].contains('.') {
        return Err(AuthError::Validation("invalid email syntax".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canon_is_lowercase() {
        assert_eq!(canonicalize_username("Alice"), "alice");
        assert_eq!(canonicalize_username("ALICE"), "alice");
    }

    #[test]
    fn canon_is_idempotent() {
        let x = canonicalize_username("MixedCase_42");
        assert_eq!(canonicalize_username(&x), x);
    }

    #[test]
    fn valid_usernames_pass() {
        for s in ["alice", "Alice", "user_42", "with-hyphen", "ab"] {
            validate_username(s).expect(s);
        }
    }

    #[test]
    fn invalid_usernames_fail() {
        for s in ["a", "this_username_is_too_long_yes", "user@host", "user space", "-bad", "bad-", "double--hyphen"] {
            assert!(validate_username(s).is_err(), "{s} should be rejected");
        }
    }

    #[test]
    fn reserved_names_rejected() {
        for r in ["me", "admin", "users", "auth", "ME", "Admin"] {
            assert!(validate_username(r).is_err(), "{r} should be reserved");
        }
    }

    #[test]
    fn email_validator() {
        validate_email("a@b.com").unwrap();
        validate_email("alice.smith@example.org").unwrap();
        assert!(validate_email("no-at-sign.com").is_err());
        assert!(validate_email("@nolocal.com").is_err());
        assert!(validate_email("nodomain@").is_err());
        assert!(validate_email("noTld@host").is_err());
    }
}
