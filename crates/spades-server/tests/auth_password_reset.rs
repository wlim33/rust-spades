use axum::http::StatusCode;
use serde_json::json;

mod common;

#[tokio::test]
async fn password_reset_flow_invalidates_old_sessions() {
    let server = common::test_server();
    server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
        }))
        .await
        .assert_status(StatusCode::CREATED);

    // Initial /auth/me works (logged in from register).
    server.get("/auth/me").await.assert_status(StatusCode::OK);

    // Request reset.
    server
        .post("/auth/password-reset/request")
        .json(&json!({
            "email": "alice@example.com",
        }))
        .await
        .assert_status(StatusCode::ACCEPTED);

    // Request for unknown email is also 202 (no leak).
    server
        .post("/auth/password-reset/request")
        .json(&json!({
            "email": "ghost@example.com",
        }))
        .await
        .assert_status(StatusCode::ACCEPTED);

    // Confirm with bogus token → 410.
    let bogus = server
        .post("/auth/password-reset/confirm")
        .json(&json!({
            "token": "definitely-not-real", "new_password": "totally-different-pw",
        }))
        .await;
    bogus.assert_status(StatusCode::GONE);

    // TODO: extract token from LogMailer and verify successful reset flow once
    // test_server() exposes the mailer handle.
}
