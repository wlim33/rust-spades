use axum::extract::{Path, Query, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{AuthError, AuthState, AuthUser};

#[derive(Serialize)]
pub struct PublicProfile {
    pub username: String,
    pub created_at: String,
    pub games_played: i64,
    pub last_seen_at: Option<String>,
}

pub async fn get_profile(
    State(auth): State<AuthState>,
    Path(username): Path<String>,
) -> Result<Json<PublicProfile>, AuthError> {
    let user = auth.store.find_user_by_username(&username).map_err(AuthError::Storage)?
        .ok_or(AuthError::NotFound)?;
    let games_played = auth.store.count_game_seats_for_user(user.id).map_err(AuthError::Storage)?;
    Ok(Json(PublicProfile {
        username: user.username,
        created_at: user.created_at,
        games_played,
        last_seen_at: user.last_login_at,
    }))
}

#[derive(Deserialize)]
pub struct GamesPagination {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}
fn default_limit() -> i64 { 20 }

#[derive(Serialize)]
pub struct ProfileGames {
    pub username: String,
    pub limit: i64,
    pub offset: i64,
    pub total: i64,
    pub games: Vec<ProfileGameEntry>,
}

#[derive(Serialize)]
pub struct ProfileGameEntry {
    pub game_id: Uuid,
    pub seat_index: i32,
    pub player_id: Uuid,
}

pub async fn get_profile_games(
    State(auth): State<AuthState>,
    Path(username): Path<String>,
    Query(p): Query<GamesPagination>,
) -> Result<Json<ProfileGames>, AuthError> {
    let user = auth.store.find_user_by_username(&username).map_err(AuthError::Storage)?
        .ok_or(AuthError::NotFound)?;
    let total = auth.store.count_game_seats_for_user(user.id).map_err(AuthError::Storage)?;
    let rows = auth.store.game_seats_for_user(user.id, p.limit.clamp(1, 100), p.offset.max(0))
        .map_err(AuthError::Storage)?;
    Ok(Json(ProfileGames {
        username: user.username,
        limit: p.limit,
        offset: p.offset,
        total,
        games: rows.into_iter().map(|r| ProfileGameEntry {
            game_id: r.game_id,
            seat_index: r.seat_index,
            player_id: r.player_id,
        }).collect(),
    }))
}

use crate::auth::tokens::{generate_token, hash_token, PURPOSE_VERIFY_EMAIL};
use crate::auth::mailer::Email;
use crate::auth::password::{hash_password, validate_password, verify_password};
use crate::auth::session_ext;
use crate::auth::users::validate_email;
use tower_sessions::Session;

#[derive(Deserialize)]
pub struct PatchMeRequest {
    #[serde(default)] pub email: Option<String>,
    #[serde(default)] pub current_password: Option<String>,
    #[serde(default)] pub new_password: Option<String>,
}

pub async fn patch_me(
    State(auth): State<AuthState>,
    session: Session,
    AuthUser(user): AuthUser,
    Json(req): Json<PatchMeRequest>,
) -> Result<Json<crate::handlers_auth::UserResponse>, AuthError> {
    // === Validate first, mutate second ===

    if let Some(ref new_email) = req.email {
        validate_email(new_email)?;
    }

    if let Some(ref new_password) = req.new_password {
        validate_password(new_password)?;
        let current = req.current_password.as_deref()
            .ok_or_else(|| AuthError::Validation("current_password required for password change".into()))?;
        let phc = user.password_hash.as_deref()
            .ok_or_else(|| AuthError::Validation("OAuth-only accounts cannot set password here".into()))?;
        if !verify_password(current, phc)? {
            return Err(AuthError::InvalidCredentials);
        }
    }

    // === Mutations: now we know all inputs are valid ===

    if let Some(new_email) = req.email.as_deref() {
        let new_version = auth.store.update_user_email(user.id, new_email).map_err(|e| match e.as_str() {
            "email_taken" => AuthError::EmailTaken,
            other => AuthError::Storage(other.into()),
        })?;
        // Re-stamp THIS session's token_version so the user stays logged in (other sessions invalidated).
        session_ext::set_claimed(&session, user.id, new_version).await?;
        let token = generate_token();
        let h = hash_token(&token);
        auth.store.insert_auth_token(&h, user.id, PURPOSE_VERIFY_EMAIL, 24 * 3600)
            .map_err(AuthError::Storage)?;
        let link = format!("{}/auth/verify-email?token={}", auth.oauth.redirect_base_url, token);
        let _ = auth.mailer.send(Email {
            to: new_email.to_string(),
            subject: "Verify your new email".into(),
            body: format!("Verify: {link}"),
        }).await;
    }

    if let Some(new_password) = req.new_password.as_deref() {
        let new_hash = hash_password(new_password)?;
        let new_version = auth.store.update_user_password(user.id, &new_hash).map_err(AuthError::Storage)?;
        session_ext::set_claimed(&session, user.id, new_version).await?;
    }

    let updated = auth.store.find_user_by_id(user.id).map_err(AuthError::Storage)?
        .ok_or_else(|| AuthError::Internal("user vanished after update".into()))?;
    Ok(Json(crate::handlers_auth::UserResponse::from(&updated)))
}
