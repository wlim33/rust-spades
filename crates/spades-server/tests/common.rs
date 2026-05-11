//! Shared test scaffolding for auth integration tests.

use axum::{extract::connect_info::MockConnectInfo, routing::{get, post}, Router};
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

pub fn test_server() -> TestServer {
    let store = Arc::new(SqliteStore::open(":memory:").unwrap());
    let auth = AuthState {
        store: store.clone(),
        mailer: Arc::new(LogMailer::new()),
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
        .with_state(auth);

    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(time::Duration::days(1)));

    let app = router
        .layer(session_layer)
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
    TestServer::new_with_config(app, TestServerConfig {
        save_cookies: true,
        ..Default::default()
    }).unwrap()
}
