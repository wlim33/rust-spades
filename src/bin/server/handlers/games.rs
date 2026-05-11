use axum::{
    extract::{Path, State as AxumState},
    http::StatusCode,
    response::Json,
};
use oasgen::oasgen;
use spades::game_manager::{
    CreateGameResponse, GameManagerError, GameStateResponse, HandResponse,
};
use spades::validation::validate_player_name;
use spades::{GameTransition, decode_player_url, short_id_to_uuid, uuid_to_short_id};
use uuid::Uuid;

use super::super::dto::{
    CreateGameRequest, ErrorResponse, PlayerUrlResponse, PresenceSnapshot,
    SetNameRequest, TransitionRequest, TransitionResponse, TransitionType,
};
use super::super::{parse_uuid_or_short_id, AppState};

pub async fn root() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "Spades Game Server",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": {
            "create_game": "POST /games",
            "list_games": "GET /games",
            "get_game_state": "GET /games/:game_id",
            "make_transition": "POST /games/:game_id/transition",
            "get_hand": "GET /games/:game_id/players/:player_id/hand",
            "set_player_name": "PUT /games/:game_id/players/:player_id/name",
            "game_ws": "GET /games/:game_id/ws?player_id=<uuid>",
            "delete_game": "DELETE /games/:game_id",
            "seek": "POST /matchmaking/seek",
            "list_seeks": "GET /matchmaking/seeks",
            "queue_sizes": "GET /matchmaking/queue-sizes",
        }
    }))
}

#[oasgen]
pub async fn create_game(
    AxumState(state): AxumState<AppState>,
    Json(request): Json<CreateGameRequest>,
) -> Result<Json<CreateGameResponse>, (StatusCode, Json<ErrorResponse>)> {
    let map_err = |e: GameManagerError| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("{}", e),
            }),
        )
    };

    match request.num_humans {
        None | Some(4) => {
            let response = state
                .game_manager
                .create_game(request.max_points, request.timer_config)
                .map_err(map_err)?;
            state.presence.ensure_game(response.game_id, &response.player_ids);
            Ok(Json(response))
        }
        Some(1) | Some(2) => {
            let num = request.num_humans.unwrap();
            let human_seats: std::collections::HashSet<usize> = match num {
                1 => [0].into_iter().collect(),
                _ => [0, 2].into_iter().collect(),
            };

            let strategy = std::sync::Arc::new(spades::ai::RandomStrategy);
            let response = state
                .game_manager
                .create_ai_game(human_seats.clone(), request.max_points, request.timer_config, strategy)
                .map_err(map_err)?;

            let game_id = response.game_id;
            state.presence.ensure_game(game_id, &response.player_ids);
            for i in 0..4 {
                if !human_seats.contains(&i) {
                    state.presence.player_connected(game_id, response.player_ids[i]);
                }
            }

            state
                .game_manager
                .make_transition(game_id, GameTransition::Start)
                .map_err(map_err)?;
            state.game_manager.play_ai_turns(game_id).map_err(map_err)?;

            Ok(Json(response))
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "num_humans must be 1, 2, or 4".to_string(),
            }),
        )),
    }
}

#[oasgen]
pub async fn list_games(
    AxumState(state): AxumState<AppState>,
) -> Result<Json<Vec<Uuid>>, (StatusCode, Json<ErrorResponse>)> {
    state.game_manager.list_games().map(Json).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("{}", e),
            }),
        )
    })
}

#[oasgen]
pub async fn get_game_state(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
) -> Result<Json<GameStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .game_manager
        .get_game_state(game_id)
        .map(Json)
        .map_err(|e| {
            let status = match e {
                GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
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
pub async fn get_game_by_short_id_handler(
    AxumState(state): AxumState<AppState>,
    Path(short_id): Path<String>,
) -> Result<Json<GameStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    let game_id = short_id_to_uuid(&short_id).ok_or((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: "Game not found".to_string(),
        }),
    ))?;
    state
        .game_manager
        .get_game_state(game_id)
        .map(Json)
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Game not found".to_string(),
                }),
            )
        })
}

#[oasgen]
pub async fn get_game_by_player_url(
    AxumState(state): AxumState<AppState>,
    Path(url_id): Path<String>,
) -> Result<Json<PlayerUrlResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (game_id, player_id) = decode_player_url(&url_id).ok_or((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse { error: "Invalid player URL".to_string() }),
    ))?;
    let game = state.game_manager.get_game_state(game_id).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse { error: "Game not found".to_string() }),
    ))?;
    let hand = state.game_manager.get_hand(game_id, player_id).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse { error: "Player not found in game".to_string() }),
    ))?;
    Ok(Json(PlayerUrlResponse {
        game_id,
        player_id,
        player_short_id: uuid_to_short_id(player_id),
        game,
        hand,
    }))
}

pub async fn delete_game(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let result = state
        .game_manager
        .remove_game(game_id)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| {
            let status = match e {
                GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("{}", e),
                }),
            )
        })?;
    state.presence.remove_game(game_id);
    Ok(result)
}

#[oasgen]
pub async fn make_transition(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
    Json(request): Json<TransitionRequest>,
) -> Result<Json<TransitionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let transition = match request.transition {
        TransitionType::Start => GameTransition::Start,
        TransitionType::Bet { amount } => GameTransition::Bet(amount),
        TransitionType::Card { card } => GameTransition::Card(card),
    };

    let result = state
        .game_manager
        .make_transition(game_id, transition)
        .map_err(|e| {
            let status = match e {
                GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_REQUEST,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("{}", e),
                }),
            )
        })?;

    let _ = state.game_manager.play_ai_turns(game_id);

    Ok(Json(TransitionResponse {
        success: true,
        result: format!("{:?}", result),
    }))
}

#[oasgen]
pub async fn get_hand(
    AxumState(state): AxumState<AppState>,
    Path((game_id, player_id_raw)): Path<(Uuid, String)>,
) -> Result<Json<HandResponse>, (StatusCode, Json<ErrorResponse>)> {
    let player_id = parse_uuid_or_short_id(&player_id_raw).ok_or((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse { error: "Invalid player ID".to_string() }),
    ))?;
    state
        .game_manager
        .get_hand(game_id, player_id)
        .map(Json)
        .map_err(|e| {
            let status = match e {
                GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_REQUEST,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("{}", e),
                }),
            )
        })
}

pub async fn set_player_name(
    AxumState(state): AxumState<AppState>,
    Path((game_id, player_id_raw)): Path<(Uuid, String)>,
    Json(request): Json<SetNameRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let player_id = parse_uuid_or_short_id(&player_id_raw).ok_or((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse { error: "Invalid player ID".to_string() }),
    ))?;
    let validated_name = match request.name {
        Some(raw) => Some(validate_player_name(&raw).map_err(|e| {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
        })?),
        None => None,
    };

    state
        .game_manager
        .set_player_name(game_id, player_id, validated_name)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| {
            let status = match e {
                GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_REQUEST,
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
pub async fn get_presence(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
) -> Result<Json<PresenceSnapshot>, (StatusCode, Json<ErrorResponse>)> {
    if state.presence.get_snapshot(game_id).is_none() {
        let game_state = state.game_manager.get_game_state(game_id).map_err(|e| {
            let status = match e {
                GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse { error: format!("{}", e) }))
        })?;
        let player_ids: Vec<Uuid> =
            game_state.player_names.iter().map(|pn| pn.player_id).collect();
        state.presence.ensure_game(game_id, &player_ids);
    }
    state.presence.get_snapshot(game_id).map(Json).ok_or((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse { error: "Game not found".to_string() }),
    ))
}
