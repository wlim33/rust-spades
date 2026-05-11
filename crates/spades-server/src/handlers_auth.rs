use axum::{extract::{ConnectInfo, Query, State}, response::{Json, Redirect}};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::{
    AuthError, AuthState, AuthUser,
    mailer::Email,
    password::{hash_password, validate_password, verify_password, verify_against_dummy},
    rate_limit::{check_email, check_ip},
    session_ext,
    tokens::{generate_token, hash_token, PURPOSE_PASSWORD_RESET, PURPOSE_VERIFY_EMAIL},
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    session: Session,
    Json(req): Json<RegisterRequest>,
) -> Result<(axum::http::StatusCode, Json<UserResponse>), AuthError> {
    check_ip(&auth.rate.register, addr.ip())?;
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

    let token = generate_token();
    let token_hash = hash_token(&token);
    auth.store.insert_auth_token(&token_hash, user_id, PURPOSE_VERIFY_EMAIL, 24 * 3600)
        .map_err(AuthError::Storage)?;
    let link = format!("{}/auth/verify-email?token={}", auth.oauth.redirect_base_url, token);
    let _ = auth.mailer.send(Email {
        to: user.email.clone(),
        subject: "Verify your Spades email".into(),
        body: format!("Verify your email: {link}\n\nThis link expires in 24 hours."),
    }).await;

    Ok((axum::http::StatusCode::CREATED, Json(UserResponse::from(&user))))
}


#[derive(Deserialize)]
pub struct LoginRequest {
    pub login: String,
    pub password: String,
}

pub async fn login(
    State(auth): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    session: Session,
    Json(req): Json<LoginRequest>,
) -> Result<Json<UserResponse>, AuthError> {
    check_ip(&auth.rate.login, addr.ip())?;
    let user_opt = if req.login.contains('@') {
        auth.store.find_user_by_email(&req.login).map_err(AuthError::Storage)?
    } else {
        auth.store.find_user_by_username(&req.login).map_err(AuthError::Storage)?
    };

    let Some(user) = user_opt else {
        verify_against_dummy();
        return Err(AuthError::InvalidCredentials);
    };

    if let Some(locked_until) = auth.store.get_lockout(user.id).map_err(AuthError::Storage)? {
        let now = chrono::Utc::now().naive_utc();
        if let Ok(when) = chrono::NaiveDateTime::parse_from_str(&locked_until, "%Y-%m-%d %H:%M:%S") {
            if when > now {
                let secs = (when - now).num_seconds().max(1) as u64;
                return Err(AuthError::Locked { retry_after_secs: secs });
            }
        }
    }

    let Some(hash) = user.password_hash.as_deref() else {
        verify_against_dummy();
        return Err(AuthError::InvalidCredentials);
    };

    if !verify_password(&req.password, hash)? {
        let new_count = auth.store.bump_login_failure(user.id).map_err(AuthError::Storage)?;
        let lock_secs = match new_count {
            n if n >= 10 => Some(60 * 60),
            n if n >= 5  => Some(15 * 60),
            _ => None,
        };
        if let Some(secs) = lock_secs {
            auth.store.set_lockout(user.id, secs).map_err(AuthError::Storage)?;
            return Err(AuthError::Locked { retry_after_secs: secs as u64 });
        }
        return Err(AuthError::InvalidCredentials);
    }

    auth.store.clear_login_failures(user.id).map_err(AuthError::Storage)?;
    auth.store.touch_user_login(user.id).map_err(AuthError::Storage)?;
    let s = session_ext::load_or_init(&session).await?;
    auth.store.claim_anon_game_seats(s.user_id, user.id).map_err(AuthError::Storage)?;
    session_ext::set_claimed(&session, user.id, user.token_version).await?;

    Ok(Json(UserResponse::from(&user)))
}

pub async fn logout(session: Session) -> Result<axum::http::StatusCode, AuthError> {
    session_ext::clear_claimed(&session).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn me(AuthUser(user): AuthUser) -> Json<UserResponse> {
    Json(UserResponse::from(&user))
}

#[derive(Deserialize)]
pub struct VerifyEmailQuery {
    pub token: String,
}

pub async fn verify_email(
    State(auth): State<AuthState>,
    Query(q): Query<VerifyEmailQuery>,
) -> Result<Redirect, AuthError> {
    let hash = hash_token(&q.token);
    let consumed = auth.store.consume_auth_token(&hash, PURPOSE_VERIFY_EMAIL)
        .map_err(|e| if e == "token_invalid" { AuthError::TokenInvalid } else { AuthError::Storage(e) })?;
    auth.store.set_user_email_verified(consumed.user_id).map_err(AuthError::Storage)?;
    Ok(Redirect::to("/"))
}

#[derive(Deserialize)]
pub struct PasswordResetRequestBody {
    pub email: String,
}

pub async fn password_reset_request(
    State(auth): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<PasswordResetRequestBody>,
) -> Result<axum::http::StatusCode, AuthError> {
    check_ip(&auth.rate.password_reset_request_ip, addr.ip())?;
    check_email(&auth.rate.password_reset_request_email, &req.email)?;
    if let Some(user) = auth.store.find_user_by_email(&req.email).map_err(AuthError::Storage)? {
        let token = generate_token();
        let hash = hash_token(&token);
        auth.store.insert_auth_token(&hash, user.id, PURPOSE_PASSWORD_RESET, 3600)
            .map_err(AuthError::Storage)?;
        let link = format!("{}/auth/password-reset?token={}", auth.oauth.redirect_base_url, token);
        let _ = auth.mailer.send(Email {
            to: user.email,
            subject: "Reset your Spades password".into(),
            body: format!("Reset link: {link}\n\nExpires in 1 hour."),
        }).await;
    }
    Ok(axum::http::StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
pub struct PasswordResetConfirmBody {
    pub token: String,
    pub new_password: String,
}

pub async fn password_reset_confirm(
    State(auth): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    session: Session,
    Json(req): Json<PasswordResetConfirmBody>,
) -> Result<axum::http::StatusCode, AuthError> {
    check_ip(&auth.rate.password_reset_confirm, addr.ip())?;
    validate_password(&req.new_password)?;
    let hash = hash_token(&req.token);
    let consumed = auth.store.consume_auth_token(&hash, PURPOSE_PASSWORD_RESET)
        .map_err(|e| if e == "token_invalid" { AuthError::TokenInvalid } else { AuthError::Storage(e) })?;
    let new_hash = hash_password(&req.new_password)?;
    let new_version = auth.store.update_user_password(consumed.user_id, &new_hash)
        .map_err(AuthError::Storage)?;
    session_ext::set_claimed(&session, consumed.user_id, new_version).await?;
    Ok(axum::http::StatusCode::OK)
}
