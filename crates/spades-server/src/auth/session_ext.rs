//! Typed helpers over the tower-sessions session blob.
//!
//! Defines the canonical `UserSession` type. The binary re-exports it via
//! `bin/server/dto.rs::UserSession` so existing imports keep working.

use crate::auth::AuthError;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use uuid::Uuid;

pub const SESSION_USER_KEY: &str = "user";

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UserSession {
    pub user_id: Uuid,
    #[serde(default)]
    pub display_name: Option<String>,
    /// Set when the session belongs to a registered, logged-in user.
    #[serde(default)]
    pub claimed_by: Option<Uuid>,
    /// Snapshot of `users.token_version` at the time `claimed_by` was set.
    #[serde(default)]
    pub token_version: i32,
}

/// Read the session user. Mint a fresh anonymous one if absent.
pub async fn load_or_init(session: &Session) -> Result<UserSession, AuthError> {
    if let Some(s) = session
        .get::<UserSession>(SESSION_USER_KEY)
        .await
        .map_err(|e| AuthError::Internal(format!("session get: {e}")))?
    {
        return Ok(s);
    }
    let fresh = UserSession {
        user_id: Uuid::new_v4(),
        ..Default::default()
    };
    session
        .insert(SESSION_USER_KEY, fresh.clone())
        .await
        .map_err(|e| AuthError::Internal(format!("session insert: {e}")))?;
    Ok(fresh)
}

/// Write the session user back.
pub async fn save(session: &Session, user: &UserSession) -> Result<(), AuthError> {
    session
        .insert(SESSION_USER_KEY, user.clone())
        .await
        .map_err(|e| AuthError::Internal(format!("session save: {e}")))?;
    Ok(())
}

/// Set `claimed_by` and `token_version` (i.e., mark the session as logged in).
pub async fn set_claimed(
    session: &Session,
    user_id: Uuid,
    token_version: i32,
) -> Result<UserSession, AuthError> {
    let mut s = load_or_init(session).await?;
    s.claimed_by = Some(user_id);
    s.token_version = token_version;
    save(session, &s).await?;
    Ok(s)
}

/// Clear `claimed_by` and `token_version` (i.e., log out). Preserves `user_id` (anon identity).
pub async fn clear_claimed(session: &Session) -> Result<(), AuthError> {
    let mut s = load_or_init(session).await?;
    s.claimed_by = None;
    s.token_version = 0;
    save(session, &s).await?;
    Ok(())
}
