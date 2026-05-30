use axum::http::StatusCode;
use serde_json::json;
mod common;

#[tokio::test]
async fn logout_clears_claim_but_keeps_session() {
    let server = common::test_server();
    server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
        }))
        .await
        .assert_status(StatusCode::CREATED);

    // While logged in, /auth/me works.
    server.get("/auth/me").await.assert_status(StatusCode::OK);

    // Log out.
    server
        .post("/auth/logout")
        .await
        .assert_status(StatusCode::NO_CONTENT);

    // /auth/me now 401.
    server
        .get("/auth/me")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    // Logging back in with the correct password should succeed — the session
    // cookie is preserved and claimed_by gets set again.
    server
        .post("/auth/login")
        .json(&json!({
            "login": "alice@example.com", "password": "hunter2-strong",
        }))
        .await
        .assert_status(StatusCode::OK);

    server.get("/auth/me").await.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn logout_is_idempotent() {
    let server = common::test_server();
    server
        .post("/auth/register")
        .json(&json!({
            "username": "Bob", "email": "bob@example.com", "password": "hunter2-strong",
        }))
        .await
        .assert_status(StatusCode::CREATED);

    // Logout once → NO_CONTENT.
    server
        .post("/auth/logout")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    // Logout again (already logged out) → still NO_CONTENT.
    server
        .post("/auth/logout")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    // Still 401 after double logout.
    server
        .get("/auth/me")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}
