use axum::{extract::State, response::Json};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::{
    AuthError, AuthState,
    mailer::Email,
    password::{hash_password, validate_password},
    session_ext,
    users::{validate_email, validate_username, NewUser, User},
};

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
}

impl From<&User> for UserResponse {
    fn from(u: &User) -> Self {
        UserResponse {
            id: u.id,
            username: u.username.clone(),
            email: u.email.clone(),
            email_verified: u.email_verified,
        }
    }
}

pub async fn register(
    State(auth): State<AuthState>,
    session: Session,
    Json(req): Json<RegisterRequest>,
) -> Result<(axum::http::StatusCode, Json<UserResponse>), AuthError> {
    let username = validate_username(&req.username)?;
    validate_email(&req.email)?;
    validate_password(&req.password)?;

    let hash = hash_password(&req.password)?;
    let new = NewUser {
        username,
        email: req.email.clone(),
        password_hash: Some(hash),
        email_verified: false,
    };

    let user_id = auth.store.insert_user(&new).map_err(|e| match e.as_str() {
        "username_taken" => AuthError::UsernameTaken,
        "email_taken" => AuthError::EmailTaken,
        other => AuthError::Storage(other.to_string()),
    })?;

    let s = session_ext::load_or_init(&session).await?;
    let anon_id = s.user_id;
    auth.store.claim_anon_game_seats(anon_id, user_id).map_err(AuthError::Storage)?;

    let user = auth.store.find_user_by_id(user_id).map_err(AuthError::Storage)?
        .ok_or_else(|| AuthError::Internal("user vanished after insert".into()))?;
    session_ext::set_claimed(&session, user_id, user.token_version).await?;

    let token = generate_email_token();
    let token_hash = sha256_hex(&token);
    auth.store.insert_auth_token(&token_hash, user_id, "verify_email", 24 * 3600)
        .map_err(AuthError::Storage)?;
    let link = format!("{}/auth/verify-email?token={}", auth.oauth.redirect_base_url, token);
    let _ = auth.mailer.send(Email {
        to: user.email.clone(),
        subject: "Verify your Spades email".into(),
        body: format!("Verify your email: {link}\n\nThis link expires in 24 hours."),
    }).await;

    Ok((axum::http::StatusCode::CREATED, Json(UserResponse::from(&user))))
}

fn generate_email_token() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

fn sha256_hex(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(s.as_bytes());
    hex::encode(h)
}
