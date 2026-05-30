//! API-token endpoint tests (bot account flow).
//!
//! Auth-gated CRUD over `/auth/tokens`. The happy path (mint + use + revoke)
//! is covered in `bin/server/main.rs::bot_account_bearer_token_authenticates_as_owner`
//! against the full router; here we focus on the validation and authorization
//! branches that are otherwise unreachable from the bin's smoke test.

use axum::http::StatusCode;
use serde_json::json;

mod common;

async fn register_alice(server: &axum_test::TestServer) {
    let resp = server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice",
            "email": "alice@example.com",
            "password": "hunter2-strong",
        }))
        .await;
    resp.assert_status(StatusCode::CREATED);
}

#[tokio::test]
async fn create_token_requires_authenticated_user() {
    let server = common::test_server();
    let resp = server
        .post("/auth/tokens")
        .json(&json!({"name": "anon"}))
        .await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_tokens_requires_authenticated_user() {
    let server = common::test_server();
    let resp = server.get("/auth/tokens").await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn revoke_token_requires_authenticated_user() {
    let server = common::test_server();
    let resp = server
        .delete(&format!("/auth/tokens/{}", uuid::Uuid::new_v4()))
        .await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_token_rejects_empty_name() {
    let server = common::test_server();
    register_alice(&server).await;
    let resp = server
        .post("/auth/tokens")
        .json(&json!({"name": "   "}))
        .await;
    resp.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"], "validation");
}

#[tokio::test]
async fn create_token_rejects_long_name() {
    let server = common::test_server();
    register_alice(&server).await;
    let resp = server
        .post("/auth/tokens")
        .json(&json!({"name": "x".repeat(101)}))
        .await;
    resp.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn revoke_token_returns_404_for_unknown_token() {
    let server = common::test_server();
    register_alice(&server).await;
    let resp = server
        .delete(&format!("/auth/tokens/{}", uuid::Uuid::new_v4()))
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn revoke_token_404_when_token_belongs_to_other_user() {
    // Alice creates a token; Bob tries to revoke it → server treats it as
    // NotFound (rather than Forbidden) to avoid leaking the existence of
    // tokens owned by other users.
    let server = common::test_server();
    register_alice(&server).await;
    let create = server
        .post("/auth/tokens")
        .json(&json!({"name": "alice-bot"}))
        .await;
    create.assert_status(StatusCode::CREATED);
    let token_id = create.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    server
        .post("/auth/logout")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    server
        .post("/auth/register")
        .json(&json!({
            "username": "Bob", "email": "bob@example.com", "password": "hunter2-strong",
        }))
        .await
        .assert_status(StatusCode::CREATED);

    let resp = server.delete(&format!("/auth/tokens/{token_id}")).await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_tokens_returns_empty_for_user_with_none() {
    let server = common::test_server();
    register_alice(&server).await;
    let resp = server.get("/auth/tokens").await;
    resp.assert_status(StatusCode::OK);
    let body: Vec<serde_json::Value> = resp.json();
    assert_eq!(body.len(), 0);
}

#[tokio::test]
async fn list_tokens_shows_minted_token_without_plaintext() {
    let server = common::test_server();
    register_alice(&server).await;
    let create = server
        .post("/auth/tokens")
        .json(&json!({"name": "alice-bot"}))
        .await;
    create.assert_status(StatusCode::CREATED);
    let plaintext = create.json::<serde_json::Value>()["token"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = server.get("/auth/tokens").await;
    resp.assert_status(StatusCode::OK);
    let body: Vec<serde_json::Value> = resp.json();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["name"], "alice-bot");
    // Plaintext must not be returned in list responses — only at mint time.
    assert!(body[0].get("token").is_none());
    // ...and the plaintext was at least non-trivial.
    assert!(plaintext.len() > 16);
}
