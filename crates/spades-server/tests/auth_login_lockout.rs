use axum::http::StatusCode;
use serde_json::json;
mod common;

#[tokio::test]
async fn five_wrong_passwords_locks_account() {
    let server = common::test_server();
    server.post("/auth/register").json(&json!({
        "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
    })).await.assert_status(StatusCode::CREATED);
    server.post("/auth/logout").await;

    // 4 wrong passwords: each returns 401 (under the lockout threshold).
    for _ in 0..4 {
        let r = server.post("/auth/login").json(&json!({
            "login": "alice@example.com", "password": "wrong",
        })).await;
        r.assert_status(StatusCode::UNAUTHORIZED);
    }

    // 5th wrong → 423 Locked.
    let r = server.post("/auth/login").json(&json!({
        "login": "alice@example.com", "password": "wrong",
    })).await;
    r.assert_status(StatusCode::LOCKED);
    let body: serde_json::Value = r.json();
    assert_eq!(body["error"], "locked");
    assert!(body["retry_after_secs"].as_u64().unwrap() > 0);

    // Even the correct password during lockout → 423.
    let r = server.post("/auth/login").json(&json!({
        "login": "alice@example.com", "password": "hunter2-strong",
    })).await;
    r.assert_status(StatusCode::LOCKED);
}
