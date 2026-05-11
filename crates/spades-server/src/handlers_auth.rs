use axum::{extract::{ConnectInfo, Path, Query, State}, response::{Json, Redirect}};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::{
    AuthError, AuthState, AuthUser,
    mailer::Email,
    oauth::{google_client, github_client, PendingSignup},
    password::{hash_password, validate_password, verify_password, verify_against_dummy},
    rate_limit::{check_email, check_ip},
    session_ext,
    tokens::{generate_token, hash_token, PURPOSE_PASSWORD_RESET, PURPOSE_VERIFY_EMAIL},
    users::{validate_email, validate_username, NewUser, User},
};
use crate::lock_util::MutexExt;

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

// ---------------------------------------------------------------------------
// OAuth handlers
// ---------------------------------------------------------------------------

use oauth2::{AuthorizationCode, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, Scope, TokenResponse};

pub async fn oauth_login(
    State(auth): State<AuthState>,
    session: Session,
    Path(provider): Path<String>,
) -> Result<Redirect, AuthError> {
    let s = session_ext::load_or_init(&session).await?;
    let anon_id = s.user_id;

    let client = match provider.as_str() {
        "google" => google_client(&auth.oauth),
        "github" => github_client(&auth.oauth),
        _ => return Err(AuthError::OauthFailed("unknown provider".into())),
    }.ok_or_else(|| AuthError::OauthFailed("provider not configured".into()))?;

    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let scopes: Vec<Scope> = match provider.as_str() {
        "google" => vec![Scope::new("openid".into()), Scope::new("email".into()), Scope::new("profile".into())],
        "github" => vec![Scope::new("read:user".into()), Scope::new("user:email".into())],
        _ => vec![],
    };
    let (auth_url, csrf_token) = client.authorize_url(CsrfToken::new_random)
        .add_scopes(scopes)
        .set_pkce_challenge(challenge)
        .url();

    auth.oauth.csrf.lock_or_recover().insert(
        csrf_token.secret().clone(),
        (anon_id, time::OffsetDateTime::now_utc() + time::Duration::minutes(10), verifier.secret().to_string()),
    );

    Ok(Redirect::to(auth_url.as_str()))
}

#[derive(Deserialize)]
pub struct OauthCallbackQuery {
    pub code: String,
    pub state: String,
}

#[derive(Deserialize)]
struct GoogleUserinfo {
    sub: String,
    email: String,
    #[serde(default)]
    email_verified: bool,
    #[serde(default)]
    name: Option<String>,
}

pub async fn oauth_google_callback(
    State(auth): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    session: Session,
    Query(q): Query<OauthCallbackQuery>,
) -> Result<axum::response::Response, AuthError> {
    use axum::response::IntoResponse as _;
    check_ip(&auth.rate.oauth_callback, addr.ip())?;

    let entry = auth.oauth.csrf.lock_or_recover().remove(&q.state);
    let (anon_from_csrf, _expires, verifier_secret) = entry.ok_or_else(|| AuthError::OauthFailed("invalid state".into()))?;
    let client = google_client(&auth.oauth)
        .ok_or_else(|| AuthError::OauthFailed("google not configured".into()))?;

    let verifier = PkceCodeVerifier::new(verifier_secret);
    let token = client.exchange_code(AuthorizationCode::new(q.code))
        .set_pkce_verifier(verifier)
        .request_async(oauth2::reqwest::async_http_client).await
        .map_err(|e| AuthError::OauthFailed(format!("token exchange: {e}")))?;

    let info: GoogleUserinfo = reqwest::Client::new()
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(token.access_token().secret())
        .send().await.map_err(|e| AuthError::OauthFailed(format!("userinfo fetch: {e}")))?
        .error_for_status().map_err(|e| AuthError::OauthFailed(format!("userinfo status: {e}")))?
        .json().await.map_err(|e| AuthError::OauthFailed(format!("userinfo parse: {e}")))?;

    if let Some(uid) = auth.store.find_oauth_account("google", &info.sub).map_err(AuthError::Storage)? {
        let user = auth.store.find_user_by_id(uid).map_err(AuthError::Storage)?
            .ok_or_else(|| AuthError::Internal("oauth account refs missing user".into()))?;
        oauth_claim_and_login(&session, &auth, anon_from_csrf, &user).await?;
        return Ok(Redirect::to("/").into_response());
    }

    if info.email_verified {
        if let Some(existing) = auth.store.find_user_by_email(&info.email).map_err(AuthError::Storage)? {
            if existing.email_verified {
                auth.store.insert_oauth_account("google", &info.sub, existing.id, &info.email)
                    .map_err(AuthError::Storage)?;
                oauth_claim_and_login(&session, &auth, anon_from_csrf, &existing).await?;
                return Ok(Redirect::to("/").into_response());
            }
        }
    }

    // Pending signup
    let temp_id = generate_token();
    let suggested = info.name.clone()
        .map(|n| n.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(20).collect::<String>())
        .filter(|s| s.len() >= 2)
        .unwrap_or_else(|| "user".into());
    let expires_at = time::OffsetDateTime::now_utc() + time::Duration::minutes(15);
    auth.oauth.pending.lock_or_recover().insert(temp_id.clone(), PendingSignup {
        provider: "google".into(),
        provider_uid: info.sub,
        email: info.email,
        email_verified: info.email_verified,
        suggested_username: suggested,
        expires_at,
    });
    let cookie = format!("__oauth_pending={temp_id}; Max-Age=900; HttpOnly; SameSite=Lax; Path=/");
    let mut resp = Redirect::to("/").into_response();
    resp.headers_mut().append(axum::http::header::SET_COOKIE, cookie.parse().unwrap());
    Ok(resp)
}

async fn oauth_claim_and_login(
    session: &Session,
    auth: &AuthState,
    anon_from_csrf: Uuid,
    user: &User,
) -> Result<(), AuthError> {
    let live = session_ext::load_or_init(session).await?;
    let anon = live.user_id;
    auth.store.claim_anon_game_seats(anon, user.id).map_err(AuthError::Storage)?;
    if anon != anon_from_csrf {
        auth.store.claim_anon_game_seats(anon_from_csrf, user.id).map_err(AuthError::Storage)?;
    }
    session_ext::set_claimed(session, user.id, user.token_version).await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct OauthCompleteRequest {
    pub username: String,
}

pub async fn oauth_complete(
    State(auth): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    session: Session,
    cookie_jar: axum_extra::extract::CookieJar,
    Json(req): Json<OauthCompleteRequest>,
) -> Result<(axum::http::StatusCode, Json<UserResponse>), AuthError> {
    check_ip(&auth.rate.oauth_callback, addr.ip())?;
    let temp_id = cookie_jar.get("__oauth_pending")
        .ok_or(AuthError::TokenInvalid)?
        .value().to_string();
    let pending = auth.oauth.pending.lock_or_recover().remove(&temp_id)
        .ok_or(AuthError::TokenInvalid)?;
    if pending.expires_at < time::OffsetDateTime::now_utc() {
        return Err(AuthError::TokenInvalid);
    }
    let username = validate_username(&req.username)?;

    let user_id = auth.store.insert_user(&NewUser {
        username,
        email: pending.email.clone(),
        password_hash: None,
        email_verified: pending.email_verified,
    }).map_err(|e| match e.as_str() {
        "username_taken" => AuthError::UsernameTaken,
        "email_taken" => AuthError::EmailTaken,
        other => AuthError::Storage(other.into()),
    })?;
    auth.store.insert_oauth_account(&pending.provider, &pending.provider_uid, user_id, &pending.email)
        .map_err(AuthError::Storage)?;

    let s = session_ext::load_or_init(&session).await?;
    auth.store.claim_anon_game_seats(s.user_id, user_id).map_err(AuthError::Storage)?;
    let user = auth.store.find_user_by_id(user_id).map_err(AuthError::Storage)?
        .ok_or_else(|| AuthError::Internal("user vanished".into()))?;
    session_ext::set_claimed(&session, user_id, user.token_version).await?;

    Ok((axum::http::StatusCode::CREATED, Json(UserResponse::from(&user))))
}

#[derive(Deserialize)]
struct GithubUser {
    id: u64,
    login: String,
}

#[derive(Deserialize)]
struct GithubEmail {
    email: String,
    primary: bool,
    verified: bool,
}

pub async fn oauth_github_callback(
    State(auth): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    session: Session,
    Query(q): Query<OauthCallbackQuery>,
) -> Result<axum::response::Response, AuthError> {
    use axum::response::IntoResponse as _;
    check_ip(&auth.rate.oauth_callback, addr.ip())?;

    let entry = auth.oauth.csrf.lock_or_recover().remove(&q.state);
    let (anon_from_csrf, _expires, verifier_secret) = entry.ok_or_else(|| AuthError::OauthFailed("invalid state".into()))?;
    let client = github_client(&auth.oauth)
        .ok_or_else(|| AuthError::OauthFailed("github not configured".into()))?;

    let verifier = PkceCodeVerifier::new(verifier_secret);
    let token = client.exchange_code(AuthorizationCode::new(q.code))
        .set_pkce_verifier(verifier)
        .request_async(oauth2::reqwest::async_http_client).await
        .map_err(|e| AuthError::OauthFailed(format!("token exchange: {e}")))?;

    let http = reqwest::Client::new();
    let user: GithubUser = http.get("https://api.github.com/user")
        .bearer_auth(token.access_token().secret())
        .header("User-Agent", "rust-spades")
        .send().await.map_err(|e| AuthError::OauthFailed(format!("user fetch: {e}")))?
        .error_for_status().map_err(|e| AuthError::OauthFailed(format!("user status: {e}")))?
        .json().await.map_err(|e| AuthError::OauthFailed(format!("user parse: {e}")))?;
    let emails: Vec<GithubEmail> = http.get("https://api.github.com/user/emails")
        .bearer_auth(token.access_token().secret())
        .header("User-Agent", "rust-spades")
        .send().await.map_err(|e| AuthError::OauthFailed(format!("emails fetch: {e}")))?
        .error_for_status().map_err(|e| AuthError::OauthFailed(format!("emails status: {e}")))?
        .json().await.map_err(|e| AuthError::OauthFailed(format!("emails parse: {e}")))?;
    let primary = emails.iter().find(|e| e.primary)
        .ok_or_else(|| AuthError::OauthFailed("no primary email".into()))?;

    if let Some(uid) = auth.store.find_oauth_account("github", &user.id.to_string()).map_err(AuthError::Storage)? {
        let u = auth.store.find_user_by_id(uid).map_err(AuthError::Storage)?
            .ok_or_else(|| AuthError::Internal("oauth account refs missing".into()))?;
        oauth_claim_and_login(&session, &auth, anon_from_csrf, &u).await?;
        return Ok(Redirect::to("/").into_response());
    }
    if primary.verified {
        if let Some(existing) = auth.store.find_user_by_email(&primary.email).map_err(AuthError::Storage)? {
            if existing.email_verified {
                auth.store.insert_oauth_account("github", &user.id.to_string(), existing.id, &primary.email)
                    .map_err(AuthError::Storage)?;
                oauth_claim_and_login(&session, &auth, anon_from_csrf, &existing).await?;
                return Ok(Redirect::to("/").into_response());
            }
        }
    }

    let temp_id = generate_token();
    let suggested: String = user.login.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').take(20).collect();
    let expires_at = time::OffsetDateTime::now_utc() + time::Duration::minutes(15);
    auth.oauth.pending.lock_or_recover().insert(temp_id.clone(), PendingSignup {
        provider: "github".into(),
        provider_uid: user.id.to_string(),
        email: primary.email.clone(),
        email_verified: primary.verified,
        suggested_username: suggested,
        expires_at,
    });
    let mut resp = Redirect::to("/").into_response();
    let cookie = format!("__oauth_pending={temp_id}; Max-Age=900; HttpOnly; SameSite=Lax; Path=/");
    resp.headers_mut().append(axum::http::header::SET_COOKIE, cookie.parse().unwrap());
    Ok(resp)
}
