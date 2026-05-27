use axum::{
    extract::{Path, State as AxumState},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        Json,
    },
};
use oasgen::oasgen;
use spades_server::challenges::{
    ChallengeConfig, ChallengeError, ChallengeEvent, ChallengeManager, ChallengeStatus,
    ChallengeSummary, Seat,
};
use spades_server::matchmaking::MatchResult;
use spades_server::validation::validate_player_name;
use std::convert::Infallible;
use std::time::Duration;
use uuid::Uuid;

use super::super::dto::{
    ErrorResponse, JoinChallengeRequest,
};
use super::super::AppState;

struct ChallengeGuard {
    challenge_manager: ChallengeManager,
    challenge_id: Uuid,
    seat: Seat,
    player_id: Uuid,
    game_started: bool,
}

impl Drop for ChallengeGuard {
    fn drop(&mut self) {
        if !self.game_started {
            self.challenge_manager
                .vacate_seat(self.challenge_id, self.seat, self.player_id);
        }
    }
}

pub async fn create_challenge_handler(
    AxumState(state): AxumState<AppState>,
    identity: spades_server::auth::Identity,
    Json(mut config): Json<ChallengeConfig>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    spades_server::auth::rate_limit::check_user(&state.auth.rate.challenge_action, identity.anon_id())
        .map_err(super::super::dto::auth_err_response)?;

    if config.max_points <= 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "max_points must be positive".to_string(),
            }),
        ));
    }

    config.creator_name = match config.creator_name {
        Some(raw) => Some(validate_player_name(&raw).map_err(|e| {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
        })?),
        None => None,
    };

    config.creator_name = if let Some(user) = identity.user() {
        Some(user.username.clone())
    } else {
        config.creator_name
    };

    let identity_user = identity.user().map(|u| u.id);
    let anon = identity.anon_id();
    let store = state.auth.store.clone();

    let creator_seat = config.creator_seat;
    let (challenge_id, creator_player_id, mut rx) = state
        .challenge_manager
        .create_challenge_with_owner(config, Some(anon), identity_user)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("{}", e),
                }),
            )
        })?;

    let status = state.challenge_manager.get_status(challenge_id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("{}", e),
            }),
        )
    })?;

    let challenge_manager = state.challenge_manager.clone();
    let stream = async_stream::stream! {
        let mut _guard = creator_seat.map(|seat| ChallengeGuard {
            challenge_manager: challenge_manager.clone(),
            challenge_id,
            seat,
            player_id: creator_player_id.unwrap_or(Uuid::nil()),
            game_started: false,
        });

        yield Ok(Event::default()
            .event("challenge_created")
            .json_data(serde_json::json!({
                "challenge_id": challenge_id,
                "short_id": status.short_id,
                "creator_player_id": creator_player_id,
                "seats": status.seats,
                "join_urls": {
                    "A": format!("/challenges/{}/join/A", challenge_id),
                    "B": format!("/challenges/{}/join/B", challenge_id),
                    "C": format!("/challenges/{}/join/C", challenge_id),
                    "D": format!("/challenges/{}/join/D", challenge_id),
                },
                "expires_at_epoch_secs": status.expires_at_epoch_secs,
            }))
            .unwrap());

        loop {
            match rx.recv().await {
                Ok(ChallengeEvent::SeatUpdate { challenge_id: cid, seats }) => {
                    yield Ok(Event::default()
                        .event("seat_update")
                        .json_data(serde_json::json!({
                            "challenge_id": cid,
                            "seats": seats,
                        }))
                        .unwrap());
                }
                Ok(ChallengeEvent::GameStart(result)) => {
                    if let Some(guard) = _guard.as_mut() {
                        guard.game_started = true;
                    }
                    if let Some(pid) = creator_player_id {
                        if let Some(seat_index) = result.player_ids.iter().position(|p| *p == pid) {
                            let _ = store.insert_game_seat(
                                result.game_id, seat_index as i32, pid,
                                spades_server::auth::game_seats::SeatOwner {
                                    user_id: identity_user,
                                    anon_user_id: Some(anon),
                                    is_bot: false,
                                },
                            );
                        }
                    }
                    let pid = creator_player_id.unwrap_or(Uuid::nil());
                    let personalized = MatchResult {
                        game_id: result.game_id,
                        player_id: pid,
                        player_short_id: spades::uuid_to_short_id(pid),
                        player_url: spades::encode_player_url(result.game_id, pid),
                        player_ids: result.player_ids,
                        player_names: result.player_names.clone(),
                        short_id: result.short_id.clone(),
                    };
                    yield Ok(Event::default()
                        .event("game_start")
                        .json_data(&personalized)
                        .unwrap());
                    break;
                }
                Ok(ChallengeEvent::Cancelled { challenge_id: cid, reason }) => {
                    yield Ok(Event::default()
                        .event("cancelled")
                        .json_data(serde_json::json!({
                            "challenge_id": cid,
                            "reason": reason,
                        }))
                        .unwrap());
                    break;
                }
                Ok(ChallengeEvent::ChallengeCreated { .. }) => continue,
                Err(_) => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}

#[oasgen]
pub async fn list_challenges_handler(
    AxumState(state): AxumState<AppState>,
) -> Json<Vec<ChallengeSummary>> {
    Json(state.challenge_manager.list_challenges())
}

#[oasgen]
pub async fn get_challenge_handler(
    AxumState(state): AxumState<AppState>,
    Path(challenge_id): Path<Uuid>,
) -> Result<Json<ChallengeStatus>, (StatusCode, Json<ErrorResponse>)> {
    state
        .challenge_manager
        .get_status(challenge_id)
        .map(Json)
        .map_err(|e| {
            let status = match e {
                ChallengeError::NotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("{}", e),
                }),
            )
        })
}

#[oasgen]
pub async fn get_challenge_by_short_id_handler(
    AxumState(state): AxumState<AppState>,
    Path(short_id): Path<String>,
) -> Result<Json<ChallengeStatus>, (StatusCode, Json<ErrorResponse>)> {
    match state.challenge_manager.get_challenge_by_short_id(&short_id) {
        Ok(status) => Ok(Json(status)),
        Err(ChallengeError::NotFound) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Challenge not found".to_string(),
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("{}", e),
            }),
        )),
    }
}

pub async fn join_challenge_handler(
    AxumState(state): AxumState<AppState>,
    identity: spades_server::auth::Identity,
    Path((challenge_id, seat_str)): Path<(Uuid, String)>,
    body: Option<Json<JoinChallengeRequest>>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    spades_server::auth::rate_limit::check_user(&state.auth.rate.challenge_action, identity.anon_id())
        .map_err(super::super::dto::auth_err_response)?;

    let seat: Seat = seat_str.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid seat. Must be A, B, C, or D".to_string(),
            }),
        )
    })?;

    let validated_name = match body.and_then(|b| b.0.name) {
        Some(raw) => Some(validate_player_name(&raw).map_err(|e| {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
        })?),
        None => None,
    };

    let validated_name = if let Some(user) = identity.user() {
        Some(user.username.clone())
    } else {
        validated_name
    };

    let identity_user = identity.user().map(|u| u.id);
    let anon = identity.anon_id();
    let store = state.auth.store.clone();

    let (player_id, mut rx) = state
        .challenge_manager
        .join_challenge(challenge_id, seat, validated_name)
        .await
        .map_err(|e| {
            let status = match &e {
                ChallengeError::NotFound => StatusCode::NOT_FOUND,
                ChallengeError::SeatTaken => StatusCode::CONFLICT,
                ChallengeError::NotOpen => StatusCode::GONE,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("{}", e),
                }),
            )
        })?;

    let challenge_manager = state.challenge_manager.clone();
    let stream = async_stream::stream! {
        let mut guard = ChallengeGuard {
            challenge_manager,
            challenge_id,
            seat,
            player_id,
            game_started: false,
        };

        yield Ok(Event::default()
            .event("joined")
            .json_data(serde_json::json!({
                "challenge_id": challenge_id,
                "seat": seat,
                "player_id": player_id,
            }))
            .unwrap());

        loop {
            match rx.recv().await {
                Ok(ChallengeEvent::SeatUpdate { challenge_id: cid, seats }) => {
                    yield Ok(Event::default()
                        .event("seat_update")
                        .json_data(serde_json::json!({
                            "challenge_id": cid,
                            "seats": seats,
                        }))
                        .unwrap());
                }
                Ok(ChallengeEvent::GameStart(result)) => {
                    guard.game_started = true;
                    if let Some(seat_index) = result.player_ids.iter().position(|p| *p == player_id) {
                        let _ = store.insert_game_seat(
                            result.game_id, seat_index as i32, player_id,
                            spades_server::auth::game_seats::SeatOwner {
                                user_id: identity_user,
                                anon_user_id: Some(anon),
                                is_bot: false,
                            },
                        );
                    }
                    let personalized = MatchResult {
                        game_id: result.game_id,
                        player_id,
                        player_short_id: spades::uuid_to_short_id(player_id),
                        player_url: spades::encode_player_url(result.game_id, player_id),
                        player_ids: result.player_ids,
                        player_names: result.player_names.clone(),
                        short_id: result.short_id.clone(),
                    };
                    yield Ok(Event::default()
                        .event("game_start")
                        .json_data(&personalized)
                        .unwrap());
                    break;
                }
                Ok(ChallengeEvent::Cancelled { challenge_id: cid, reason }) => {
                    yield Ok(Event::default()
                        .event("cancelled")
                        .json_data(serde_json::json!({
                            "challenge_id": cid,
                            "reason": reason,
                        }))
                        .unwrap());
                    break;
                }
                Ok(ChallengeEvent::ChallengeCreated { .. }) => continue,
                Err(_) => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}

pub async fn cancel_challenge_handler(
    AxumState(state): AxumState<AppState>,
    Path(challenge_id): Path<Uuid>,
    identity: spades_server::auth::Identity,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    state
        .challenge_manager
        .cancel_challenge_by_identity(
            challenge_id,
            identity.anon_id(),
            identity.user().map(|u| u.id),
        )
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| {
            let status = match &e {
                ChallengeError::NotFound => StatusCode::NOT_FOUND,
                ChallengeError::NotCreator => StatusCode::FORBIDDEN,
                ChallengeError::NotOpen => StatusCode::GONE,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("{}", e),
                }),
            )
        })
}
