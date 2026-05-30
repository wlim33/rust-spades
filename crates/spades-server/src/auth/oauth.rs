//! Google + GitHub OAuth flow, state CSRF, PKCE, pending-signup store.

use crate::lock_util::MutexExt;
use std::collections::HashMap;
use std::sync::Mutex;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct OauthProviderConfig {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Default)]
pub struct OauthState {
    pub google: Option<OauthProviderConfig>,
    pub github: Option<OauthProviderConfig>,
    pub redirect_base_url: String,
    /// state -> (anon_id, expires_at, pkce_verifier_secret).
    /// We store the verifier as a String because oauth2::PkceCodeVerifier is not Clone.
    pub csrf: Mutex<HashMap<String, (Uuid, OffsetDateTime, String)>>,
    pub pending: Mutex<HashMap<String, PendingSignup>>,
}

#[derive(Clone, Debug)]
pub struct PendingSignup {
    pub provider: String,
    pub provider_uid: String,
    pub email: String,
    pub email_verified: bool,
    pub suggested_username: String,
    pub expires_at: OffsetDateTime,
}

use oauth2::basic::BasicClient;
use oauth2::{AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};

pub fn google_client(state: &OauthState) -> Option<BasicClient> {
    let cfg = state.google.as_ref()?;
    let auth = AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".into()).ok()?;
    let token = TokenUrl::new("https://oauth2.googleapis.com/token".into()).ok()?;
    let redirect = RedirectUrl::new(format!(
        "{}/auth/oauth/google/callback",
        state.redirect_base_url
    ))
    .ok()?;
    Some(
        BasicClient::new(
            ClientId::new(cfg.client_id.clone()),
            Some(ClientSecret::new(cfg.client_secret.clone())),
            auth,
            Some(token),
        )
        .set_redirect_uri(redirect),
    )
}

pub fn github_client(state: &OauthState) -> Option<BasicClient> {
    let cfg = state.github.as_ref()?;
    let auth = AuthUrl::new("https://github.com/login/oauth/authorize".into()).ok()?;
    let token = TokenUrl::new("https://github.com/login/oauth/access_token".into()).ok()?;
    let redirect = RedirectUrl::new(format!(
        "{}/auth/oauth/github/callback",
        state.redirect_base_url
    ))
    .ok()?;
    Some(
        BasicClient::new(
            ClientId::new(cfg.client_id.clone()),
            Some(ClientSecret::new(cfg.client_secret.clone())),
            auth,
            Some(token),
        )
        .set_redirect_uri(redirect),
    )
}

impl OauthState {
    /// Drop entries whose `expires_at` is in the past from both `csrf` and
    /// `pending`. Returns the total number of entries removed.
    ///
    /// Without this, every `/auth/<provider>/login` hit leaks one csrf entry
    /// (and OAuth-init that lands on the signup-pending path leaks one
    /// pending entry) until the process restarts. Drive-by attackers
    /// hitting the login URL turn this into an unbounded `HashMap` —
    /// `sweep_expired` is meant to be called periodically from the
    /// background.
    pub fn sweep_expired(&self) -> usize {
        let now = OffsetDateTime::now_utc();
        let mut removed = 0;
        {
            let mut csrf = self.csrf.lock_or_recover();
            let before = csrf.len();
            csrf.retain(|_, (_, expires_at, _)| *expires_at > now);
            removed += before - csrf.len();
        }
        {
            let mut pending = self.pending.lock_or_recover();
            let before = pending.len();
            pending.retain(|_, p| p.expires_at > now);
            removed += before - pending.len();
        }
        removed
    }

    pub fn from_env() -> Self {
        let google = match (
            std::env::var("GOOGLE_OAUTH_CLIENT_ID"),
            std::env::var("GOOGLE_OAUTH_CLIENT_SECRET"),
        ) {
            (Ok(id), Ok(sec)) => Some(OauthProviderConfig {
                client_id: id,
                client_secret: sec,
            }),
            _ => None,
        };
        let github = match (
            std::env::var("GITHUB_OAUTH_CLIENT_ID"),
            std::env::var("GITHUB_OAUTH_CLIENT_SECRET"),
        ) {
            (Ok(id), Ok(sec)) => Some(OauthProviderConfig {
                client_id: id,
                client_secret: sec,
            }),
            _ => None,
        };
        let redirect_base_url = std::env::var("OAUTH_REDIRECT_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
        OauthState {
            google,
            github,
            redirect_base_url,
            csrf: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration as TimeDuration;

    fn state() -> OauthState {
        OauthState::default()
    }

    fn pending(expires_at: OffsetDateTime) -> PendingSignup {
        PendingSignup {
            provider: "google".into(),
            provider_uid: "uid".into(),
            email: "a@example.com".into(),
            email_verified: false,
            suggested_username: "Alice".into(),
            expires_at,
        }
    }

    #[test]
    fn sweep_expired_removes_expired_csrf_only() {
        let s = state();
        let past = OffsetDateTime::now_utc() - TimeDuration::seconds(60);
        let future = OffsetDateTime::now_utc() + TimeDuration::seconds(60);
        s.csrf
            .lock_or_recover()
            .insert("expired".into(), (Uuid::nil(), past, "v".into()));
        s.csrf
            .lock_or_recover()
            .insert("fresh".into(), (Uuid::nil(), future, "v".into()));
        assert_eq!(s.sweep_expired(), 1);
        let csrf = s.csrf.lock_or_recover();
        assert_eq!(csrf.len(), 1);
        assert!(csrf.contains_key("fresh"));
    }

    #[test]
    fn sweep_expired_removes_expired_pending_only() {
        let s = state();
        let past = OffsetDateTime::now_utc() - TimeDuration::seconds(60);
        let future = OffsetDateTime::now_utc() + TimeDuration::seconds(60);
        s.pending
            .lock_or_recover()
            .insert("expired".into(), pending(past));
        s.pending
            .lock_or_recover()
            .insert("fresh".into(), pending(future));
        assert_eq!(s.sweep_expired(), 1);
        let pending_map = s.pending.lock_or_recover();
        assert_eq!(pending_map.len(), 1);
        assert!(pending_map.contains_key("fresh"));
    }

    #[test]
    fn sweep_expired_counts_across_both_maps() {
        let s = state();
        let past = OffsetDateTime::now_utc() - TimeDuration::seconds(60);
        s.csrf
            .lock_or_recover()
            .insert("c".into(), (Uuid::nil(), past, "v".into()));
        s.pending
            .lock_or_recover()
            .insert("p".into(), pending(past));
        assert_eq!(s.sweep_expired(), 2);
    }

    #[test]
    fn google_client_builds_when_provider_configured() {
        let s = OauthState {
            google: Some(OauthProviderConfig {
                client_id: "id".into(),
                client_secret: "sec".into(),
            }),
            redirect_base_url: "https://example.com".into(),
            ..Default::default()
        };
        assert!(google_client(&s).is_some());
    }

    #[test]
    fn google_client_none_when_provider_missing() {
        let s = state();
        assert!(google_client(&s).is_none());
    }

    #[test]
    fn github_client_builds_when_provider_configured() {
        let s = OauthState {
            github: Some(OauthProviderConfig {
                client_id: "id".into(),
                client_secret: "sec".into(),
            }),
            redirect_base_url: "https://example.com".into(),
            ..Default::default()
        };
        assert!(github_client(&s).is_some());
    }

    #[test]
    fn github_client_none_when_provider_missing() {
        let s = state();
        assert!(github_client(&s).is_none());
    }

    #[test]
    fn sweep_expired_is_a_noop_when_nothing_expired() {
        let s = state();
        let future = OffsetDateTime::now_utc() + TimeDuration::seconds(60);
        s.csrf
            .lock_or_recover()
            .insert("c".into(), (Uuid::nil(), future, "v".into()));
        s.pending
            .lock_or_recover()
            .insert("p".into(), pending(future));
        assert_eq!(s.sweep_expired(), 0);
        assert_eq!(s.csrf.lock_or_recover().len(), 1);
        assert_eq!(s.pending.lock_or_recover().len(), 1);
    }
}
