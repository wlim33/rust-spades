use axum::{
    extract::{Path, State as AxumState},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use oasgen::oasgen;
use serde::Serialize;
use spades::{GameTransition, decode_player_url, short_id_to_uuid, uuid_to_short_id};
use spades_server::game_manager::{
    CreateGameResponse, GameManagerError, GameStateResponse, HandResponse,
};
use spades_server::validation::validate_player_name;
use uuid::Uuid;

/// JSON replay response for a terminal game — available at
/// `GET /games/{game_id}/replay.json`.
#[derive(Debug, Serialize, oasgen::OaSchema)]
pub struct GameReplayResponse {
    pub model: spades::transcript::Model,
    /// Cumulative `[team_a, team_b]` score after each fully-played round.
    /// Each inner array always has exactly 2 elements.
    pub cumulative_by_round: Vec<Vec<i32>>,
    /// Seat index (0..4) the authenticated caller played, if any; else null.
    pub viewer_seat: Option<usize>,
}

use super::super::dto::{
    ChatRequest, CreateGameRequest, ErrorResponse, PlayerUrlResponse, PresenceSnapshot,
    SetNameRequest, TransitionRequest, TransitionResponse, TransitionType,
};
use super::super::idempotency::CachedOutcome;
use super::super::{AppState, parse_uuid_or_short_id};

pub async fn root() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "Spades Game Server",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": {
            "create_game": "POST /games",
            "get_game_state": "GET /games/:game_id",
            "make_transition": "POST /games/:game_id/transition",
            "get_hand": "GET /games/:game_id/players/:player_id/hand",
            "set_player_name": "PUT /games/:game_id/players/:player_id/name",
            "game_ws": "GET /games/:game_id/ws?player_id=<uuid> (seat owner only)",
            "delete_game": "DELETE /games/:game_id",
            "seek": "POST /matchmaking/seek",
            "list_seeks": "GET /matchmaking/seeks",
            "queue_sizes": "GET /matchmaking/queue-sizes",
        }
    }))
}

pub async fn create_game(
    AxumState(state): AxumState<AppState>,
    identity: spades_server::auth::Identity,
    Json(request): Json<CreateGameRequest>,
) -> Result<Json<CreateGameResponse>, (StatusCode, Json<ErrorResponse>)> {
    spades_server::auth::rate_limit::check_user(&state.auth.rate.create_game, identity.anon_id())
        .map_err(super::super::dto::auth_err_response)?;

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
            state
                .presence
                .ensure_game(response.game_id, &response.player_ids);
            let identity_user = identity.user().map(|u| u.id);
            let anon = identity.anon_id();
            for (i, pid) in response.player_ids.iter().enumerate() {
                let _ = state.auth.store.insert_game_seat(
                    response.game_id,
                    i as i32,
                    *pid,
                    spades_server::auth::game_seats::SeatOwner {
                        user_id: identity_user,
                        anon_user_id: Some(anon),
                        is_bot: false,
                    },
                );
            }
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
                .create_ai_game(
                    human_seats.clone(),
                    request.max_points,
                    request.timer_config,
                    strategy,
                )
                .map_err(map_err)?;

            let game_id = response.game_id;
            state.presence.ensure_game(game_id, &response.player_ids);
            for i in 0..4 {
                if !human_seats.contains(&i) {
                    state
                        .presence
                        .player_connected(game_id, response.player_ids[i]);
                }
            }

            // Name bot seats by relative position (south=0, west=1, north=2,
            // east=3) so the UI shows "West (CPU)" instead of "Seat 2". Set
            // before Start so the first state broadcast already has names.
            for (i, pid) in response.player_ids.iter().enumerate() {
                if human_seats.contains(&i) {
                    continue;
                }
                if let Some(name) = bot_seat_name(i) {
                    let _ = state
                        .game_manager
                        .set_player_name(game_id, *pid, Some(name.to_string()))
                        .await;
                }
            }

            state
                .game_manager
                .make_transition(game_id, GameTransition::Start)
                .await
                .map_err(map_err)?;

            let identity_user = identity.user().map(|u| u.id);
            let anon = identity.anon_id();
            for (i, pid) in response.player_ids.iter().enumerate() {
                let is_human = human_seats.contains(&i);
                let _ = state.auth.store.insert_game_seat(
                    game_id,
                    i as i32,
                    *pid,
                    spades_server::auth::game_seats::SeatOwner {
                        user_id: if is_human { identity_user } else { None },
                        anon_user_id: if is_human { Some(anon) } else { None },
                        is_bot: !is_human,
                    },
                );
            }

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
pub async fn get_game_state(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
) -> Result<Json<GameStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .game_manager
        .get_game_state(game_id)
        .await
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
        .await
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
        Json(ErrorResponse {
            error: "Invalid player URL".to_string(),
        }),
    ))?;
    let game = state
        .game_manager
        .get_game_state(game_id)
        .await
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Game not found".to_string(),
                }),
            )
        })?;
    let hand = state
        .game_manager
        .get_hand(game_id, player_id)
        .await
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Player not found in game".to_string(),
                }),
            )
        })?;
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
    identity: spades_server::auth::Identity,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Authorize: the caller must own at least one seat in this game.
    // Without this, any anonymous caller could delete any game by id. A DB
    // error is swallowed to `false` here (same as the prior per-seat loop):
    // the existence probe below then resolves it to 404/403.
    let owns_seat = state
        .auth
        .store
        .game_seats_for_game(game_id)
        .map(|seats| {
            seats
                .iter()
                .any(|seat| seat_matches_identity(seat, &identity))
        })
        .unwrap_or(false);
    if !owns_seat {
        // Probe game existence to distinguish "not yours" from "doesn't exist".
        if state.game_manager.get_game_state(game_id).await.is_err() {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Game not found".to_string(),
                }),
            ));
        }
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "only seated players may delete this game".to_string(),
            }),
        ));
    }

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
    identity: spades_server::auth::Identity,
    headers: HeaderMap,
    Json(request): Json<TransitionRequest>,
) -> Result<Json<TransitionResponse>, (StatusCode, Json<ErrorResponse>)> {
    spades_server::auth::rate_limit::check_user(&state.auth.rate.transition, identity.anon_id())
        .map_err(super::super::dto::auth_err_response)?;

    let user_id = identity.anon_id();
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Replay a cached outcome rather than re-running the transition. Without
    // this, a network blip that prompts the client to retry a Card play
    // would either error ("already played") on the second attempt or — if
    // the trick advanced in the meantime — silently apply the wrong card.
    if let Some(key) = &idempotency_key
        && let Some(cached) = state.idempotency.get(game_id, user_id, key)
    {
        return match cached {
            CachedOutcome::Ok(resp) => Ok(Json(resp)),
            CachedOutcome::Err(status, err) => Err((status, Json(err))),
        };
    }

    // Authorize after the idempotency replay on purpose: a retried action
    // that already executed must replay its cached outcome even though the
    // turn has since advanced past the caller. The cache is keyed by the
    // caller's identity, so a replay can only return outcomes that same
    // identity produced.
    authorize_transition(&state, game_id, &identity, &request.transition).await?;

    let transition = match request.transition {
        TransitionType::Start => GameTransition::Start,
        TransitionType::Bet { amount } => GameTransition::Bet(amount),
        TransitionType::Card { card } => GameTransition::Card(card),
    };

    let outcome: Result<TransitionResponse, (StatusCode, ErrorResponse)> = state
        .game_manager
        .make_transition(game_id, transition)
        .await
        .map(|result| TransitionResponse {
            success: true,
            result: format!("{:?}", result),
        })
        .map_err(|e| {
            let status = match e {
                GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_REQUEST,
            };
            (
                status,
                ErrorResponse {
                    error: format!("{}", e),
                },
            )
        });

    // Persist for retries — store both success and error outcomes so a retry
    // after a 4xx gets the same 4xx, not a fresh attempt that might succeed
    // against drifted state.
    if let Some(key) = idempotency_key {
        let cache_outcome = match &outcome {
            Ok(resp) => CachedOutcome::Ok(resp.clone()),
            Err((status, err)) => CachedOutcome::Err(*status, err.clone()),
        };
        state.idempotency.put(game_id, user_id, key, cache_outcome);
    }

    outcome.map(Json).map_err(|(s, e)| (s, Json(e)))
}

#[oasgen]
pub async fn get_hand(
    AxumState(state): AxumState<AppState>,
    Path((game_id, player_id_raw)): Path<(Uuid, String)>,
    identity: spades_server::auth::Identity,
) -> Result<Json<HandResponse>, (StatusCode, Json<ErrorResponse>)> {
    let player_id = parse_uuid_or_short_id(&player_id_raw).ok_or((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: "Invalid player ID".to_string(),
        }),
    ))?;

    // Authorize: only the player who owns this seat can read its hand.
    // Otherwise a "spectator" with the game URL (and player_ids exposed
    // in GameStateResponse) could read every opponent's hand. The
    // /games/by-player-url/{url_id} endpoint is the auth-free path: it
    // treats the URL itself as the bearer credential.
    let seat = state
        .auth
        .store
        .game_seat_by_player_id(game_id, player_id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: e }),
            )
        })?;
    let Some(seat) = seat else {
        // No seat for this (game, player). Distinguish "game gone" (404)
        // from "player not in this game" (400) by probing the game's
        // existence — this preserves the legacy response codes that
        // existing clients (and tests) depend on.
        if state.game_manager.get_game_state(game_id).await.is_err() {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Game not found".to_string(),
                }),
            ));
        }
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "player not seated in this game".to_string(),
            }),
        ));
    };
    if !seat_matches_identity(&seat, &identity) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "hand is private to its seat owner".to_string(),
            }),
        ));
    }

    state
        .game_manager
        .get_hand(game_id, player_id)
        .await
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

/// Default display name for a bot at `seat_index`. Seat 0 is the
/// (human) south seat; the other three are named by their position
/// relative to it so a 2v2 game also picks the right name for each
/// remaining bot.
fn bot_seat_name(seat_index: usize) -> Option<&'static str> {
    match seat_index {
        1 => Some("West (CPU)"),
        2 => Some("North (CPU)"),
        3 => Some("East (CPU)"),
        _ => None,
    }
}

/// Authorize `identity` to drive `game_id`: the caller must own at least
/// one human seat, and Bet/Card additionally require owning the seat whose
/// turn it is — transitions apply to whichever player the engine considers
/// current, so without the turn check any seated player could act for an
/// opponent (rated games included). Start is not turn-bound: any seated
/// player may kick the game off. When the current player's seat row is
/// missing the turn check is skipped (fails open within seated players)
/// rather than bricking a game with incomplete seat records.
async fn authorize_transition(
    state: &AppState,
    game_id: Uuid,
    identity: &spades_server::auth::Identity,
    transition: &TransitionType,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    // Only the current player id is needed for the turn check; this avoids
    // building and cloning a full GameStateResponse per transition. Doubles
    // as the existence probe — GameNotFound maps to 404.
    let current_player_id = state
        .game_manager
        .current_player(game_id)
        .await
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

    // One query for all ≤4 seats instead of one round-trip per seat plus a
    // separate by-player lookup for the turn check.
    let seats = state.auth.store.game_seats_for_game(game_id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;
    let owns_any = seats
        .iter()
        .any(|seat| seat_matches_identity(seat, identity));
    if !owns_any {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "only seated players may act in this game".to_string(),
            }),
        ));
    }

    if matches!(
        transition,
        TransitionType::Bet { .. } | TransitionType::Card { .. }
    ) && let Some(current_id) = current_player_id
        && let Some(seat) = seats.iter().find(|s| s.player_id == current_id)
        && !seat_matches_identity(seat, identity)
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "it is not your seat's turn".to_string(),
            }),
        ));
    }
    Ok(())
}

/// True when `identity` owns `seat` — meaning the requester is the
/// human player at that seat. A registered user matches by `user_id`;
/// an anonymous session matches by `anon_user_id`. Bot seats have
/// neither and can never be owned by a request.
pub(crate) fn seat_matches_identity(
    seat: &spades_server::auth::game_seats::SeatRow,
    identity: &spades_server::auth::Identity,
) -> bool {
    if seat.is_bot {
        return false;
    }
    if let Some(seat_user) = seat.user_id {
        return identity.user().map(|u| u.id) == Some(seat_user);
    }
    if let Some(seat_anon) = seat.anon_user_id {
        return seat_anon == identity.anon_id();
    }
    false
}

/// Maximum characters per chat message. Plenty for tactical chatter; tight
/// enough that the broadcast buffer doesn't bloat under heavy use.
const CHAT_MAX_LEN: usize = 500;

/// POST /games/:id/chat — broadcast a chat message into the game's event
/// stream. Auth-gated to the seat owner (spectators can read chat but
/// can't send), rate-limited per-user, validated for length + profanity.
pub async fn post_chat(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
    identity: spades_server::auth::Identity,
    Json(request): Json<ChatRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    spades_server::auth::rate_limit::check_user(&state.auth.rate.chat_message, identity.anon_id())
        .map_err(super::super::dto::auth_err_response)?;

    let content = request.content.trim();
    if content.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "chat message must not be empty".to_string(),
            }),
        ));
    }
    if content.chars().count() > CHAT_MAX_LEN {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("chat message exceeds {CHAT_MAX_LEN} characters"),
            }),
        ));
    }
    // rustrict is already in scope for name validation — reuse the same
    // censor for chat. `is_inappropriate` flags severe profanity / slurs;
    // milder bits pass through.
    use rustrict::CensorStr;
    if content.is_inappropriate() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "chat message rejected by content filter".to_string(),
            }),
        ));
    }

    // Auth-gate to seat owner — only players at this table can send chat.
    let seat = state
        .auth
        .store
        .game_seat_by_player_id(game_id, request.player_id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: e }),
            )
        })?;
    let Some(seat) = seat else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "player not seated in this game".to_string(),
            }),
        ));
    };
    if !seat_matches_identity(&seat, &identity) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "only seated players may send chat".to_string(),
            }),
        ));
    }

    state
        .game_manager
        .send_chat(game_id, request.player_id, content.to_string())
        .await
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
    Ok(StatusCode::ACCEPTED)
}

/// Return a JSON replay of a terminal game. Refused (403) for in-progress
/// games — the model would expose hidden hands. Resolves `viewer_seat` from
/// the auth `Identity` so clients can orient the replay to the viewer.
#[oasgen]
pub async fn get_replay_json(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
    identity: spades_server::auth::Identity,
) -> Result<Json<GameReplayResponse>, (StatusCode, Json<ErrorResponse>)> {
    let data = state
        .game_manager
        .get_replay_data(game_id)
        .await
        .map_err(|e| {
            let status = match e {
                GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("{e}"),
                }),
            )
        })?;
    let Some(data) = data else {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "replay is only available for completed or aborted games".to_string(),
            }),
        ));
    };

    // Resolve the caller's seat (if they played) from the seat roster.
    // Deliberate degrade: when the seat store is unavailable (e.g. server
    // running without --db) or the lookup errors, viewer_seat resolves to
    // None. The replay data is still returned — this is only an
    // orientation hint and its absence is not a failure.
    let viewer_seat = state
        .auth
        .store
        .game_seats_for_game(game_id)
        .ok()
        .and_then(|seats| {
            seats
                .iter()
                .find(|s| seat_matches_identity(s, &identity))
                .map(|s| s.seat_index as usize)
        });

    Ok(Json(GameReplayResponse {
        model: data.model,
        cumulative_by_round: data
            .cumulative_by_round
            .into_iter()
            .map(|[a, b]| vec![a, b])
            .collect(),
        viewer_seat,
    }))
}

/// Return a PGN-style text transcript of a finished game. Refused for
/// in-progress games — encoding the transcript mid-game would expose
/// every player's hand to anyone with the game URL.
pub async fn get_replay(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
) -> Result<(StatusCode, axum::http::HeaderMap, String), (StatusCode, Json<ErrorResponse>)> {
    let transcript = state
        .game_manager
        .get_transcript(game_id)
        .await
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
    let Some(transcript) = transcript else {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "replay is only available for completed or aborted games".to_string(),
            }),
        ));
    };
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    Ok((StatusCode::OK, headers, transcript))
}

pub async fn set_player_name(
    AxumState(state): AxumState<AppState>,
    Path((game_id, player_id_raw)): Path<(Uuid, String)>,
    identity: spades_server::auth::Identity,
    Json(request): Json<SetNameRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let player_id = parse_uuid_or_short_id(&player_id_raw).ok_or((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: "Invalid player ID".to_string(),
        }),
    ))?;
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

    // Authorize: the caller must own this seat. Registered-user seats are
    // canonical (name comes from the user); bot seats are server-named.
    // Anon-owned seats are renamable only by the matching anon session —
    // previously these were renamable by anyone with the player_id.
    let seat = state
        .auth
        .store
        .game_seat_by_player_id(game_id, player_id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: e }),
            )
        })?;
    let Some(seat) = seat else {
        if state.game_manager.get_game_state(game_id).await.is_err() {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Game not found".into(),
                }),
            ));
        }
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "player not seated in this game".into(),
            }),
        ));
    };
    if seat.is_bot {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "bot seat name is server-managed".into(),
            }),
        ));
    }
    if seat.user_id.is_some() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "seat owned by registered user; name is canonical".into(),
            }),
        ));
    }
    if !seat_matches_identity(&seat, &identity) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "only the seat owner may rename this player".into(),
            }),
        ));
    }

    state
        .game_manager
        .set_player_name(game_id, player_id, validated_name)
        .await
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
        let game_state = state
            .game_manager
            .get_game_state(game_id)
            .await
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
        let player_ids: Vec<Uuid> = game_state
            .player_names
            .iter()
            .map(|pn| pn.player_id)
            .collect();
        state.presence.ensure_game(game_id, &player_ids);
    }
    state.presence.get_snapshot(game_id).map(Json).ok_or((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: "Game not found".to_string(),
        }),
    ))
}
