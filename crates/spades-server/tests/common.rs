//! Shared test scaffolding for auth integration tests.

use axum::{routing::{get, post}, Router};
use axum_test::{TestServer, TestServerConfig};
use spades_server::{
    auth::{AuthState, mailer::LogMailer, oauth::OauthState, rate_limit::RateLimitState},
    handlers_auth,
    sqlite_store::SqliteStore,
};
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
        .with_state(auth);

    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(time::Duration::days(1)));

    let app = router.layer(session_layer);
    TestServer::new_with_config(app, TestServerConfig {
        save_cookies: true,
        ..Default::default()
    }).unwrap()
}
