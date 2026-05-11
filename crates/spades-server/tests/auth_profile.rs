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

#[tokio::test]
async fn email_change_invalidates_other_sessions_via_token_version() {
    let env = common::test_env();

    let reg: serde_json::Value = env.server.post("/auth/register").json(&json!({
        "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
    })).await.json();
    let user_id_s = reg["id"].as_str().unwrap();
    let user_id = uuid::Uuid::parse_str(user_id_s).unwrap();

    let before = env.store.find_user_by_id(user_id).unwrap().unwrap().token_version;

    env.server.patch("/users/me").json(&json!({
        "email": "alice2@example.com"
    })).await.assert_status(axum::http::StatusCode::OK);

    let after = env.store.find_user_by_id(user_id).unwrap().unwrap().token_version;
    assert!(after > before, "token_version should bump on email change");

    // The requester's session SHOULD still work (it was re-stamped).
    env.server.get("/auth/me").await.assert_status(axum::http::StatusCode::OK);
}

#[tokio::test]
async fn patch_me_invalid_password_does_not_change_email() {
    let env = common::test_env();
    env.server.post("/auth/register").json(&json!({
        "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
    })).await.assert_status(StatusCode::CREATED);

    // Try a combined change where new_password is invalid (too short).
    let resp = env.server.patch("/users/me").json(&json!({
        "email": "alice2@example.com",
        "current_password": "hunter2-strong",
        "new_password": "tiny",  // <8 chars
    })).await;
    resp.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    // Email should NOT have been changed (validation failed up front).
    let me: serde_json::Value = env.server.get("/auth/me").await.json();
    assert_eq!(me["email"], "alice@example.com");
}
