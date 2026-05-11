use axum::http::StatusCode;
use serde_json::json;
mod common;

#[tokio::test]
async fn profile_after_register() {
    let server = common::test_server();
    server.post("/auth/register").json(&json!({
        "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
    })).await.assert_status(StatusCode::CREATED);

    let resp = server.get("/users/Alice").await;
    resp.assert_status(StatusCode::OK);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["username"], "Alice");
    assert_eq!(body["games_played"], 0);
}

#[tokio::test]
async fn profile_404_for_unknown() {
    let server = common::test_server();
    let resp = server.get("/users/nobody").await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn profile_lookup_case_insensitive() {
    let server = common::test_server();
    server.post("/auth/register").json(&json!({
        "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
    })).await.assert_status(StatusCode::CREATED);

    let resp = server.get("/users/ALICE").await;
    resp.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn profile_games_list_empty() {
    let server = common::test_server();
    server.post("/auth/register").json(&json!({
        "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
    })).await.assert_status(StatusCode::CREATED);

    let resp = server.get("/users/Alice/games").await;
    resp.assert_status(StatusCode::OK);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["total"], 0);
    assert_eq!(body["games"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn patch_me_email_change_triggers_reverify() {
    let server = common::test_server();
    server.post("/auth/register").json(&json!({
        "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
    })).await.assert_status(StatusCode::CREATED);

    let resp = server.patch("/users/me").json(&json!({
        "email": "alice2@example.com"
    })).await;
    resp.assert_status(StatusCode::OK);

    // /auth/me should reflect the new email and email_verified=false.
    let me: serde_json::Value = server.get("/auth/me").await.json();
    assert_eq!(me["email"], "alice2@example.com");
    assert_eq!(me["email_verified"], false);
}

#[tokio::test]
async fn patch_me_password_change_requires_current() {
    let server = common::test_server();
    server.post("/auth/register").json(&json!({
        "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
    })).await.assert_status(StatusCode::CREATED);

    // Missing current_password → 422.
    let resp = server.patch("/users/me").json(&json!({
        "new_password": "even-stronger-pw",
    })).await;
    resp.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    // Wrong current_password → 401.
    let resp = server.patch("/users/me").json(&json!({
        "current_password": "wrong-pw",
        "new_password": "even-stronger-pw",
    })).await;
    resp.assert_status(StatusCode::UNAUTHORIZED);

    // Correct → 200.
    let resp = server.patch("/users/me").json(&json!({
        "current_password": "hunter2-strong",
        "new_password": "even-stronger-pw",
    })).await;
    resp.assert_status(StatusCode::OK);
}
