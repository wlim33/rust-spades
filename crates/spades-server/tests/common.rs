//! Shared test scaffolding for auth integration tests.
//!
//! Each integration test in `tests/` compiles this module as part of its own
//! binary and pulls in only the helpers it needs, so items that a particular
//! binary doesn't reference are correctly flagged as dead code. Allow it.
#![allow(dead_code)]

use axum::{extract::connect_info::MockConnectInfo, routing::{delete, get, patch, post}, Router};
#[allow(unused_imports)]
use axum::routing::put;
use axum_test::{TestServer, TestServerConfig};
use spades_server::{
    auth::{AuthState, mailer::LogMailer, oauth::OauthState, rate_limit::RateLimitState},
    handlers_auth,
    sqlite_store::SqliteStore,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

/// Full test environment: server + store + mailer for inspection.
pub struct TestEnv {
    pub server: TestServer,
    pub store: Arc<SqliteStore>,
    pub mailer: Arc<LogMailer>,
}

/// Build a TestEnv with access to the store and mailer.
pub fn test_env() -> TestEnv {
    let store = Arc::new(SqliteStore::open(":memory:").unwrap());
    let mailer = Arc::new(LogMailer::new());
    let auth = AuthState {
        store: store.clone(),
        mailer: mailer.clone() as Arc<dyn spades_server::auth::mailer::Mailer>,
        oauth: Arc::new(OauthState::from_env()),
        rate: Arc::new(RateLimitState::new()),
        secure_cookies: false,
    };

    let router = Router::new()
        .route("/auth/register", post(handlers_auth::register))
        .route("/auth/login", post(handlers_auth::login))
        .route("/auth/logout", post(handlers_auth::logout))
        .route("/auth/me", get(handlers_auth::me))
        .route("/auth/verify-email", get(handlers_auth::verify_email))
        .route("/auth/password-reset/request", post(handlers_auth::password_reset_request))
        .route("/auth/password-reset/confirm", post(handlers_auth::password_reset_confirm))
        .route("/auth/oauth/{provider}/login", get(handlers_auth::oauth_login))
        .route("/auth/oauth/google/callback", get(handlers_auth::oauth_google_callback))
        .route("/auth/oauth/github/callback", get(handlers_auth::oauth_github_callback))
        .route("/auth/oauth/complete", post(handlers_auth::oauth_complete))
        // API token management (bot accounts)
        .route("/auth/tokens", post(handlers_auth::create_token))
        .route("/auth/tokens", get(handlers_auth::list_tokens))
        .route("/auth/tokens/{token_id}", delete(handlers_auth::revoke_token))
        // User profile endpoints (literal /users/me must come before the wildcard)
        .route("/users/me", patch(spades_server::handlers_users::patch_me))
        .route("/users/{username}", get(spades_server::handlers_users::get_profile))
        .route("/users/{username}/games", get(spades_server::handlers_users::get_profile_games))
        .with_state(auth);

    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(time::Duration::days(1)));

    let app = router
        .layer(session_layer)
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
    let server = TestServer::new_with_config(app, TestServerConfig {
        save_cookies: true,
        ..Default::default()
    }).unwrap();

    TestEnv { server, store, mailer }
}

/// Convenience: just the TestServer when store/mailer access is not needed.
pub fn test_server() -> TestServer {
    test_env().server
}
