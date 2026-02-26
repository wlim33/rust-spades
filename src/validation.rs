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
