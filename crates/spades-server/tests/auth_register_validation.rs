//! Register-handler input-validation tests.
//!
//! Cover `validate_username` / `validate_email` / `validate_password` paths
//! plumbed through `handlers_auth::register`. The happy and duplicate-row
//! paths live in `auth_register_login_flow.rs`; this file is purely the
//! rejection grid.

use axum::http::StatusCode;
use serde_json::json;

mod common;

async fn assert_register_rejects(body: serde_json::Value, expected_detail_fragment: &str) {
    let server = common::test_server();
    let resp = server.post("/auth/register").json(&body).await;
    // AuthError::Validation maps to 422 (Unprocessable Entity); the human
    // message lives in the `details` field, with a stable `error: "validation"` code.
    resp.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let b: serde_json::Value = resp.json();
    assert_eq!(b["error"], "validation");
    assert!(
        b["details"].as_str().unwrap_or("").contains(expected_detail_fragment),
        "expected details to contain {expected_detail_fragment:?}, got {:?}",
        b["details"],
    );
}

#[tokio::test]
async fn register_rejects_username_too_short() {
    assert_register_rejects(
        json!({"username": "a", "email": "a@b.c", "password": "hunter2-strong"}),
        "username must be",
    ).await;
}

#[tokio::test]
async fn register_rejects_username_too_long() {
    assert_register_rejects(
        json!({"username": "a".repeat(21), "email": "a@b.c", "password": "hunter2-strong"}),
        "username must be",
    ).await;
}

#[tokio::test]
async fn register_rejects_username_with_invalid_chars() {
    assert_register_rejects(
        json!({"username": "alice!", "email": "a@b.c", "password": "hunter2-strong"}),
        "letters, digits",
    ).await;
}

#[tokio::test]
async fn register_rejects_username_with_leading_hyphen() {
    assert_register_rejects(
        json!({"username": "-alice", "email": "a@b.c", "password": "hunter2-strong"}),
        "hyphen",
    ).await;
}

#[tokio::test]
async fn register_rejects_reserved_username() {
    assert_register_rejects(
        json!({"username": "admin", "email": "a@b.c", "password": "hunter2-strong"}),
        "reserved",
    ).await;
}

#[tokio::test]
async fn register_rejects_email_missing_at() {
    assert_register_rejects(
        json!({"username": "alice", "email": "noatsign", "password": "hunter2-strong"}),
        "invalid email",
    ).await;
}

#[tokio::test]
async fn register_rejects_email_without_domain_tld() {
    assert_register_rejects(
        json!({"username": "alice", "email": "alice@local", "password": "hunter2-strong"}),
        "invalid email",
    ).await;
}

#[tokio::test]
async fn register_rejects_short_password() {
    assert_register_rejects(
        json!({"username": "alice", "email": "alice@example.com", "password": "short"}),
        "password",
    ).await;
}

#[tokio::test]
async fn register_rejects_duplicate_email() {
    let server = common::test_server();
    server.post("/auth/register").json(&json!({
        "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
    })).await.assert_status(StatusCode::CREATED);

    let dup = server.post("/auth/register").json(&json!({
        "username": "Bob", "email": "alice@example.com", "password": "hunter2-strong",
    })).await;
    dup.assert_status(StatusCode::CONFLICT);
    let body: serde_json::Value = dup.json();
    assert_eq!(body["error"], "email_taken");
}
