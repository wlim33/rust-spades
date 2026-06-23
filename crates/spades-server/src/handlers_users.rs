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
    /// Glicko-2 rating. Round to integer for display; the underlying
    /// stored value is a float.
    pub rating: i32,
    /// Rating deviation — lower means more confident about the rating.
    /// Round to integer for display.
    pub rd: i32,
}

pub async fn get_profile(
    State(auth): State<AuthState>,
    Path(username): Path<String>,
) -> Result<Json<PublicProfile>, AuthError> {
    let user = auth
        .store
        .find_user_by_username(&username)
        .map_err(AuthError::Storage)?
        .ok_or(AuthError::NotFound)?;
    let games_played = auth
        .store
        .count_game_seats_for_user(user.id)
        .map_err(AuthError::Storage)?;
    let rating = auth
        .store
        .get_user_rating(user.id)
        .map_err(AuthError::Storage)?
        .unwrap_or(crate::ratings::DEFAULT_RATING);
    Ok(Json(PublicProfile {
        username: user.username,
        created_at: user.created_at,
        games_played,
        last_seen_at: user.last_login_at,
        rating: rating.rating.round() as i32,
        rd: rating.rd.round() as i32,
    }))
}

#[derive(Deserialize)]
pub struct GamesPagination {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}
fn default_limit() -> i64 {
    20
}

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
    /// The profile owner's own seat — clients emphasize this player.
    pub seat_index: i32,
    pub player_id: Uuid,
    /// All four seats of the game, ordered by seat index. Seats 0 & 2 are one
    /// partnership; seats 1 & 3 are the other.
    pub players: Vec<SeatPlayer>,
    /// Outcome from the profile owner's perspective:
    /// `won` / `lost` / `tied` / `aborted` / `in_progress` / `unknown`.
    pub state: String,
    /// The owner's team score and the opponents', when the game has finished.
    pub team_score: Option<i32>,
    pub opp_score: Option<i32>,
}

/// Map a stamped `result` (+ a live-state peek for games still in the DB) to
/// the client-facing state string. A finished game has a `result`; a live one
/// has no result but a non-terminal `live_state`; anything else is unknown
/// (e.g. a game pruned before result tracking existed).
pub(crate) fn profile_state(result: Option<&str>, live_state: Option<&str>) -> &'static str {
    match result {
        Some("won") => "won",
        Some("lost") => "lost",
        Some("tied") => "tied",
        Some("aborted") => "aborted",
        Some(_) => "unknown",
        None => match live_state {
            // json_extract yields "Completed"/"Aborted" for terminal games and
            // {"Betting":n}/{"Trick":n}/"NotStarted" for live ones.
            Some("Completed") | Some("Aborted") | None => "unknown",
            Some(_) => "in_progress",
        },
    }
}

#[derive(Serialize)]
pub struct SeatPlayer {
    pub seat_index: i32,
    /// Display name: the registered username, or "Bot"/"Guest" for seats
    /// without a user account.
    pub name: String,
    pub is_bot: bool,
}

pub async fn get_profile_games(
    State(auth): State<AuthState>,
    Path(username): Path<String>,
    Query(p): Query<GamesPagination>,
) -> Result<Json<ProfileGames>, AuthError> {
    let user = auth
        .store
        .find_user_by_username(&username)
        .map_err(AuthError::Storage)?
        .ok_or(AuthError::NotFound)?;
    let total = auth
        .store
        .count_game_seats_for_user(user.id)
        .map_err(AuthError::Storage)?;
    let rows = auth
        .store
        .profile_games_for_user(user.id, p.limit.clamp(1, 100), p.offset.max(0))
        .map_err(AuthError::Storage)?;
    let mut games = Vec::with_capacity(rows.len());
    for r in rows {
        let players = auth
            .store
            .game_players_for_game(r.game_id)
            .map_err(AuthError::Storage)?
            .into_iter()
            .map(|(seat_index, username, is_bot)| SeatPlayer {
                seat_index,
                name: username
                    .unwrap_or_else(|| if is_bot { "Bot".into() } else { "Guest".into() }),
                is_bot,
            })
            .collect();
        let state = profile_state(r.result.as_deref(), r.live_state.as_deref());
        games.push(ProfileGameEntry {
            game_id: r.game_id,
            seat_index: r.seat_index,
            player_id: r.player_id,
            players,
            state: state.to_string(),
            team_score: r.team_score,
            opp_score: r.opp_score,
        });
    }
    Ok(Json(ProfileGames {
        username: user.username,
        limit: p.limit,
        offset: p.offset,
        total,
        games,
    }))
}

use crate::auth::mailer::Email;
use crate::auth::password::{hash_password, validate_password, verify_password};
use crate::auth::session_ext;
use crate::auth::tokens::{PURPOSE_VERIFY_EMAIL, generate_token, hash_token};
use crate::auth::users::validate_email;
use tower_sessions::Session;

#[derive(Deserialize)]
pub struct PatchMeRequest {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub current_password: Option<String>,
    #[serde(default)]
    pub new_password: Option<String>,
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
        let current = req.current_password.as_deref().ok_or_else(|| {
            AuthError::Validation("current_password required for password change".into())
        })?;
        let phc = user.password_hash.as_deref().ok_or_else(|| {
            AuthError::Validation("OAuth-only accounts cannot set password here".into())
        })?;
        if !verify_password(current, phc)? {
            return Err(AuthError::InvalidCredentials);
        }
    }

    // === Mutations: now we know all inputs are valid ===

    if let Some(new_email) = req.email.as_deref() {
        let new_version = auth
            .store
            .update_user_email(user.id, new_email)
            .map_err(|e| match e.as_str() {
                "email_taken" => AuthError::EmailTaken,
                other => AuthError::Storage(other.into()),
            })?;
        // Re-stamp THIS session's token_version so the user stays logged in (other sessions invalidated).
        session_ext::set_claimed(&session, user.id, new_version).await?;
        let token = generate_token();
        let h = hash_token(&token);
        auth.store
            .insert_auth_token(&h, user.id, PURPOSE_VERIFY_EMAIL, 24 * 3600)
            .map_err(AuthError::Storage)?;
        let link = format!(
            "{}/auth/verify-email?token={}",
            auth.oauth.redirect_base_url, token
        );
        let _ = auth
            .mailer
            .send(Email {
                to: new_email.to_string(),
                subject: "Verify your new email".into(),
                body: format!("Verify: {link}"),
            })
            .await;
    }

    if let Some(new_password) = req.new_password.as_deref() {
        let new_hash = hash_password(new_password)?;
        let new_version = auth
            .store
            .update_user_password(user.id, &new_hash)
            .map_err(AuthError::Storage)?;
        session_ext::set_claimed(&session, user.id, new_version).await?;
    }

    let updated = auth
        .store
        .find_user_by_id(user.id)
        .map_err(AuthError::Storage)?
        .ok_or_else(|| AuthError::Internal("user vanished after update".into()))?;
    Ok(Json(crate::handlers_auth::UserResponse::from(&updated)))
}

#[cfg(test)]
mod profile_state_tests {
    use super::profile_state;

    #[test]
    fn stamped_result_wins() {
        // A stamped result is authoritative regardless of any live state.
        assert_eq!(profile_state(Some("won"), None), "won");
        assert_eq!(profile_state(Some("lost"), Some("Completed")), "lost");
        assert_eq!(profile_state(Some("tied"), None), "tied");
        assert_eq!(profile_state(Some("aborted"), None), "aborted");
    }

    #[test]
    fn unstamped_uses_live_state() {
        // No result + a live (non-terminal) game row → in progress.
        assert_eq!(profile_state(None, Some("{\"Betting\":2}")), "in_progress");
        assert_eq!(profile_state(None, Some("{\"Trick\":0}")), "in_progress");
        assert_eq!(profile_state(None, Some("NotStarted")), "in_progress");
    }

    #[test]
    fn unstamped_without_live_row_is_unknown() {
        // Pruned/old game: no result and no surviving game row.
        assert_eq!(profile_state(None, None), "unknown");
        // Terminal live_state but somehow unstamped → still unknown, not a
        // bogus win/loss.
        assert_eq!(profile_state(None, Some("Completed")), "unknown");
        assert_eq!(profile_state(None, Some("Aborted")), "unknown");
    }
}
