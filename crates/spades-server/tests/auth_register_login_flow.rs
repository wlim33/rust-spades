use axum::http::StatusCode;
use serde_json::json;

mod common;

#[tokio::test]
async fn register_succeeds_and_logs_user_in() {
    let server = common::test_server();
    let resp = server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice",
            "email": "alice@example.com",
            "password": "hunter2-strong",
        }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["username"], "Alice");
    assert_eq!(body["email_verified"], false);

    let me = server.get("/auth/me").await;
    me.assert_status(StatusCode::OK);
    let me_body: serde_json::Value = me.json();
    assert_eq!(me_body["username"], "Alice");
}

#[tokio::test]
async fn register_rejects_duplicate_username() {
    let server = common::test_server();
    let req = json!({
        "username": "Alice",
        "email": "alice@example.com",
        "password": "hunter2-strong",
    });
    server
        .post("/auth/register")
        .json(&req)
        .await
        .assert_status(StatusCode::CREATED);

    let dup = server
        .post("/auth/register")
        .json(&json!({
            "username": "alice",
            "email": "different@example.com",
            "password": "hunter2-strong",
        }))
        .await;
    dup.assert_status(StatusCode::CONFLICT);
    let b: serde_json::Value = dup.json();
    assert_eq!(b["error"], "username_taken");
}

#[tokio::test]
async fn login_with_email_works() {
    let server = common::test_server();
    server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
        }))
        .await
        .assert_status(StatusCode::CREATED);

    server
        .post("/auth/logout")
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let login = server
        .post("/auth/login")
        .json(&json!({
            "login": "alice@example.com", "password": "hunter2-strong",
        }))
        .await;
    login.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn login_with_wrong_password_returns_401() {
    let server = common::test_server();
    server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
        }))
        .await
        .assert_status(StatusCode::CREATED);
    server.post("/auth/logout").await;

    let login = server
        .post("/auth/login")
        .json(&json!({
            "login": "alice@example.com", "password": "wrong-password",
        }))
        .await;
    login.assert_status(StatusCode::UNAUTHORIZED);
}
