use axum::http::StatusCode;
mod common;

#[tokio::test]
async fn oauth_callback_with_bad_state_returns_400() {
    let server = common::test_server();
    let resp = server
        .get("/auth/oauth/google/callback?code=fake&state=not-in-csrf-map")
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"], "oauth_failed");
}

#[tokio::test]
async fn github_callback_with_bad_state_returns_400() {
    let server = common::test_server();
    let resp = server
        .get("/auth/oauth/github/callback?code=fake&state=not-in-csrf-map")
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"], "oauth_failed");
}
