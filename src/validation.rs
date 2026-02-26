use rustrict::CensorStr;

/// Validate and sanitize a player display name.
/// Returns the trimmed name on success, or an error message.
pub fn validate_player_name(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    if trimmed.len() > 32 {
        return Err("Name must be 32 characters or fewer".to_string());
    }
    if trimmed.is_inappropriate() {
        return Err("Name contains inappropriate language".to_string());
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_name_rejected() {
        assert_eq!(
            validate_player_name(""),
            Err("Name cannot be empty".to_string())
        );
    }

    #[test]
    fn test_whitespace_only_name_rejected() {
        assert_eq!(
            validate_player_name("   "),
            Err("Name cannot be empty".to_string())
        );
    }

    #[test]
    fn test_too_long_name_rejected() {
        let long = "a".repeat(33);
        assert_eq!(
            validate_player_name(&long),
            Err("Name must be 32 characters or fewer".to_string())
        );
    }

    #[test]
    fn test_exactly_32_chars_accepted() {
        let name = "a".repeat(32);
        assert_eq!(validate_player_name(&name), Ok(name));
    }

    #[test]
    fn test_valid_name_trimmed() {
        assert_eq!(validate_player_name("  Alice  "), Ok("Alice".to_string()));
    }
}
