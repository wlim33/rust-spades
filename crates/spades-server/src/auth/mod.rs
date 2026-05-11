//! Identity layer: registered users, sessions (via tower-sessions extension),
//! OAuth (Google + GitHub), email verification, password reset, rate limiting,
//! seat-to-identity mapping.

pub mod error;
pub mod password;
pub mod users;
pub mod session_ext;
pub mod tokens;
pub mod game_seats;
pub mod mailer;
pub mod rate_limit;
pub mod oauth;

pub use error::AuthError;

use crate::auth::session_ext::load_or_init;
use crate::auth::users::User;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::sync::Arc;
use tower_sessions::Session;
use uuid::Uuid;

/// Shared auth state — wired into AppState in Phase 8.
#[derive(Clone)]
pub struct AuthState {
    pub store: Arc<crate::sqlite_store::SqliteStore>,
    pub mailer: Arc<dyn mailer::Mailer>,
    pub oauth: Arc<oauth::OauthState>,
    pub rate: Arc<rate_limit::RateLimitState>,
    pub secure_cookies: bool,
}

#[derive(Debug, Clone)]
pub enum Identity {
    Registered { user: User, anon_id: Uuid },
    Anonymous { anon_id: Uuid },
}

impl Identity {
    pub fn anon_id(&self) -> Uuid {
        match self {
            Identity::Registered { anon_id, .. } | Identity::Anonymous { anon_id } => *anon_id,
        }
    }
    pub fn user(&self) -> Option<&User> {
        match self {
            Identity::Registered { user, .. } => Some(user),
            Identity::Anonymous { .. } => None,
        }
    }
}

pub struct AuthUser(pub User);

impl<S> FromRequestParts<S> for Identity
where
    S: Send + Sync,
    AuthState: axum::extract::FromRef<S>,
{
    type Rejection = AuthError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state).await
            .map_err(|_| AuthError::Internal("session extractor failed".into()))?;
        let auth_state = <AuthState as axum::extract::FromRef<S>>::from_ref(state);
        identify(&session, &auth_state).await
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AuthState: axum::extract::FromRef<S>,
{
    type Rejection = AuthError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let identity = Identity::from_request_parts(parts, state).await?;
        match identity {
            Identity::Registered { user, .. } => Ok(AuthUser(user)),
            Identity::Anonymous { .. } => Err(AuthError::Unauthenticated),
        }
    }
}

/// Resolve the current `Identity` from a session. Drops `claimed_by` on stale token_version.
pub async fn identify(session: &Session, state: &AuthState) -> Result<Identity, AuthError> {
    let mut s = load_or_init(session).await?;
    let anon_id = s.user_id;

    let Some(claimed_id) = s.claimed_by else {
        return Ok(Identity::Anonymous { anon_id });
    };

    let user = state.store.find_user_by_id(claimed_id)
        .map_err(AuthError::Storage)?;

    let Some(user) = user else {
        s.claimed_by = None;
        s.token_version = 0;
        session_ext::save(session, &s).await?;
        return Ok(Identity::Anonymous { anon_id });
    };

    if user.token_version != s.token_version {
        s.claimed_by = None;
        s.token_version = 0;
        session_ext::save(session, &s).await?;
        return Ok(Identity::Anonymous { anon_id });
    }

    Ok(Identity::Registered { user, anon_id })
}
