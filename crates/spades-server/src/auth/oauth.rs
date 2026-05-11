//! Google + GitHub OAuth flow, state CSRF, PKCE, pending-signup store.

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
    let redirect = RedirectUrl::new(format!("{}/auth/oauth/google/callback", state.redirect_base_url)).ok()?;
    Some(BasicClient::new(
        ClientId::new(cfg.client_id.clone()),
        Some(ClientSecret::new(cfg.client_secret.clone())),
        auth,
        Some(token),
    ).set_redirect_uri(redirect))
}

pub fn github_client(state: &OauthState) -> Option<BasicClient> {
    let cfg = state.github.as_ref()?;
    let auth = AuthUrl::new("https://github.com/login/oauth/authorize".into()).ok()?;
    let token = TokenUrl::new("https://github.com/login/oauth/access_token".into()).ok()?;
    let redirect = RedirectUrl::new(format!("{}/auth/oauth/github/callback", state.redirect_base_url)).ok()?;
    Some(BasicClient::new(
        ClientId::new(cfg.client_id.clone()),
        Some(ClientSecret::new(cfg.client_secret.clone())),
        auth,
        Some(token),
    ).set_redirect_uri(redirect))
}

impl OauthState {
    pub fn from_env() -> Self {
        let google = match (std::env::var("GOOGLE_OAUTH_CLIENT_ID"), std::env::var("GOOGLE_OAUTH_CLIENT_SECRET")) {
            (Ok(id), Ok(sec)) => Some(OauthProviderConfig { client_id: id, client_secret: sec }),
            _ => None,
        };
        let github = match (std::env::var("GITHUB_OAUTH_CLIENT_ID"), std::env::var("GITHUB_OAUTH_CLIENT_SECRET")) {
            (Ok(id), Ok(sec)) => Some(OauthProviderConfig { client_id: id, client_secret: sec }),
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
