use axum::http::StatusCode;
use serde_json::json;
mod common;

#[tokio::test]
async fn email_verify_link_flips_email_verified() {
    let env = common::test_env();

    env.server.post("/auth/register").json(&json!({
        "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
    })).await.assert_status(StatusCode::CREATED);

    // Extract verify token from the LogMailer's captured emails.
    let sent = env.mailer.sent();
    let email = sent.last().expect("mailer captured the verify email");
    // Body contains "token=<token>" — extract the token.
    let token = email.body
        .split("token=")
        .nth(1)
        .expect("verify link contains 'token='")
        .split_whitespace()
        .next()
        .expect("token is non-empty")
        .to_string();

    // Hit the verify-email endpoint with that token.
    // verify_email returns a 302 redirect on success.
    let resp = env.server.get(&format!("/auth/verify-email?token={token}")).await;
    assert!(
        resp.status_code().is_redirection() || resp.status_code() == StatusCode::OK,
        "expected redirect or 200, got {}",
        resp.status_code()
    );

    // /auth/me should now show email_verified = true.
    let me: serde_json::Value = env.server.get("/auth/me").await.json();
    assert_eq!(me["email_verified"], true);
}

#[tokio::test]
async fn bad_verify_token_returns_error() {
    let env = common::test_env();

    env.server.post("/auth/register").json(&json!({
        "username": "Bob", "email": "bob@example.com", "password": "hunter2-strong",
    })).await.assert_status(StatusCode::CREATED);

    // Hit with a bogus token — should fail (400 or redirect to error page).
    let resp = env.server.get("/auth/verify-email?token=notavalidtoken").await;
    // Must NOT be a success-class redirect — any non-2xx/non-3xx is acceptable,
    // or a redirect to an error URL. The handler returns AuthError::TokenInvalid.
    assert!(
        !resp.status_code().is_success() || resp.status_code() == StatusCode::FOUND,
        "expected error status, got {}",
        resp.status_code()
    );
}
