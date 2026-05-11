use axum::http::StatusCode;
mod common;

#[tokio::test]
async fn oauth_login_returns_400_when_provider_not_configured() {
    let server = common::test_server();
    let resp = server.get("/auth/oauth/google/login").await;
    // No OAuth env vars set in tests → 400 with oauth_failed.
    resp.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn oauth_login_returns_400_for_unknown_provider() {
    let server = common::test_server();
    let resp = server.get("/auth/oauth/yahoo/login").await;
    resp.assert_status(StatusCode::BAD_REQUEST);
}
