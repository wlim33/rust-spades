use axum::{
    extract::State as AxumState,
    http::StatusCode,
    response::{
        Json,
        sse::{Event, KeepAlive, Sse},
    },
};
use oasgen::oasgen;
use spades_server::matchmaking::{Matchmaker, QueueSizeEntry, SeekEvent, SeekSummary};
use spades_server::validation::validate_player_name;
use std::convert::Infallible;
use std::time::Duration;
use uuid::Uuid;

use super::super::AppState;
use super::super::dto::{ErrorResponse, SeekRequest};

struct SeekGuard {
    matchmaker: Matchmaker,
    player_id: Uuid,
    matched: bool,
}

impl Drop for SeekGuard {
    fn drop(&mut self) {
        if !self.matched {
            self.matchmaker.cancel_seek(self.player_id);
        }
    }
}

pub async fn seek(
    AxumState(state): AxumState<AppState>,
    identity: spades_server::auth::Identity,
    Json(request): Json<SeekRequest>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<ErrorResponse>),
> {
    spades_server::auth::rate_limit::check_user(&state.auth.rate.create_seek, identity.anon_id())
        .map_err(super::super::dto::auth_err_response)?;

    if request.max_points <= 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "max_points must be positive".to_string(),
            }),
        ));
    }

    let validated_name = match request.name {
        Some(raw) => Some(validate_player_name(&raw).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
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

    // Band the seeker by their Glicko rating; anonymous/unrated -> default (Mid).
    let rating = match identity_user {
        Some(uid) => {
            store
                .get_user_rating(uid)
                .ok()
                .flatten()
                .unwrap_or(spades_server::ratings::DEFAULT_RATING)
                .rating
        }
        None => spades_server::ratings::DEFAULT_RATING.rating,
    };

    let (player_id, mut rx) = state
        .matchmaker
        .add_seek(
            request.max_points,
            request.timer_config,
            validated_name,
            rating,
        )
        .await;

    let stream = async_stream::stream! {
        let mut guard = SeekGuard {
            matchmaker: state.matchmaker.clone(),
            player_id,
            matched: false,
        };

        while let Some(event) = rx.recv().await {
            match event {
                SeekEvent::QueueUpdate { waiting } => {
                    yield Ok(Event::default()
                        .event("queue_status")
                        .json_data(serde_json::json!({
                            "position": waiting,
                            "waiting": waiting,
                        }))
                        .unwrap());
                }
                SeekEvent::GameStart(result) => {
                    guard.matched = true;
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
                    yield Ok(Event::default()
                        .event("game_start")
                        .json_data(&result)
                        .unwrap());
                    break;
                }
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
pub async fn list_seeks_handler(AxumState(state): AxumState<AppState>) -> Json<Vec<SeekSummary>> {
    Json(state.matchmaker.list_seeks())
}

#[oasgen]
pub async fn queue_sizes_handler(
    AxumState(state): AxumState<AppState>,
) -> Json<Vec<QueueSizeEntry>> {
    Json(state.matchmaker.queue_sizes())
}
