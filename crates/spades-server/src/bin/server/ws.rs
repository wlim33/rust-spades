use axum::{
    extract::{
        Path, Query, State as AxumState,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::Json,
};
use spades_server::game_manager::{GameEvent, GameStateResponse};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::dto::{ErrorResponse, PresenceSnapshot, ServerEvent, WsQuery};
use super::handlers::games::seat_matches_identity;
use super::presence::PresenceTracker;
use super::{AppState, parse_uuid_or_short_id};

pub async fn game_ws(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
    Query(query): Query<WsQuery>,
    identity: spades_server::auth::Identity,
    ws: WebSocketUpgrade,
) -> Result<impl axum::response::IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Authorize before the upgrade: the event stream is private to seated
    // players (same rule as get_hand). Without this, anyone who learns a
    // game_id can watch the game in real time. The status codes mirror
    // get_hand: 401 when no seat is claimed, 400/404 for unknown seats,
    // 403 when the seat belongs to someone else.
    let player_id = query
        .player_id
        .as_deref()
        .and_then(parse_uuid_or_short_id)
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "game stream is private; connect with the player_id of a seat you own"
                    .to_string(),
            }),
        ))?;
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
        // Distinguish "game gone" (404) from "player not in this game"
        // (400) by probing the game's existence, as get_hand does.
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
                error: "game stream is private to its seat owners".to_string(),
            }),
        ));
    }

    let sub = state
        .game_manager
        .subscribe(game_id, query.since)
        .await
        .map_err(|e| {
            let status = match e {
                spades_server::game_manager::GameManagerError::GameNotFound => {
                    StatusCode::NOT_FOUND
                }
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("{}", e),
                }),
            )
        })?;

    let player_ids: Vec<Uuid> = sub
        .initial_state
        .player_names
        .iter()
        .map(|pn| pn.player_id)
        .collect();
    state.presence.ensure_game(game_id, &player_ids);

    let presence_rx = state.presence.subscribe(game_id);
    let initial_presence = state.presence.get_snapshot(game_id);

    let presence = state.presence.clone();
    Ok(ws.on_upgrade(move |socket| {
        handle_game_ws(
            socket,
            sub.initial_state,
            sub.current_seq,
            sub.rx,
            sub.catch_up,
            player_id,
            game_id,
            presence,
            presence_rx,
            initial_presence,
        )
    }))
}

async fn handle_game_ws(
    mut socket: WebSocket,
    initial_state: GameStateResponse,
    initial_seq: u64,
    mut rx: broadcast::Receiver<GameEvent>,
    catch_up: Option<Vec<GameEvent>>,
    player_id: Uuid,
    game_id: Uuid,
    presence: PresenceTracker,
    presence_rx: Option<broadcast::Receiver<PresenceSnapshot>>,
    initial_presence: Option<PresenceSnapshot>,
) {
    // Send catch-up events (since=N path) OR a fresh snapshot. Only one,
    // never both: catch-up events are deltas applied on top of state the
    // client already has; a fresh subscriber gets the snapshot instead.
    match catch_up {
        Some(events) => {
            for event in events {
                let server_event = match event {
                    GameEvent::StateChanged { seq, state } => {
                        ServerEvent::StateChanged { seq, state }
                    }
                    GameEvent::GameAborted {
                        seq,
                        game_id,
                        reason,
                    } => ServerEvent::GameAborted {
                        seq,
                        game_id,
                        reason,
                    },
                    GameEvent::ChatMessage {
                        seq,
                        game_id,
                        player_id,
                        content,
                    } => ServerEvent::ChatMessage {
                        seq,
                        game_id,
                        player_id,
                        content,
                    },
                };
                if let Ok(json) = serde_json::to_string(&server_event) {
                    if socket.send(Message::Text(json.into())).await.is_err() {
                        return;
                    }
                }
            }
        }
        None => {
            let initial_event = ServerEvent::StateChanged {
                seq: initial_seq,
                state: initial_state,
            };
            if let Ok(json) = serde_json::to_string(&initial_event) {
                if socket.send(Message::Text(json.into())).await.is_err() {
                    return;
                }
            }
        }
    }

    if let Some(snapshot) = initial_presence {
        let event = ServerEvent::PresenceChanged(snapshot);
        if let Ok(json) = serde_json::to_string(&event) {
            if socket.send(Message::Text(json.into())).await.is_err() {
                return;
            }
        }
    }

    if let Some(snapshot) = presence.player_connected(game_id, player_id) {
        presence.broadcast(game_id, snapshot);
    }

    let mut presence_rx = presence_rx;

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(game_event) => {
                        let server_event = match game_event {
                            GameEvent::StateChanged { seq, state } => ServerEvent::StateChanged { seq, state },
                            GameEvent::GameAborted { seq, game_id, reason } => ServerEvent::GameAborted { seq, game_id, reason },
                            GameEvent::ChatMessage { seq, game_id, player_id, content } => {
                                ServerEvent::ChatMessage { seq, game_id, player_id, content }
                            }
                        };
                        if let Ok(json) = serde_json::to_string(&server_event) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        // Subscriber fell more than capacity events behind. The
                        // remaining stream would silently desync from server
                        // truth — force the client to reconnect for a fresh
                        // snapshot.
                        tracing::warn!(game_id = %game_id, dropped = n, "game ws lagged; forcing resync");
                        let resync = ServerEvent::Resync { reason: format!("lagged {n}") };
                        if let Ok(json) = serde_json::to_string(&resync) {
                            let _ = socket.send(Message::Text(json.into())).await;
                        }
                        let _ = socket.send(Message::Close(None)).await;
                        break;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        // The game actor is gone (e.g. the game was deleted),
                        // so no further events will arrive. Send a graceful
                        // close handshake instead of dropping the socket — a
                        // bare drop reaches the client as a TCP reset
                        // (`ResetWithoutClosingHandshake`) rather than a clean
                        // close.
                        let _ = socket.send(Message::Close(None)).await;
                        break;
                    }
                }
            }
            event = async {
                match presence_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                match event {
                    Ok(snapshot) => {
                        let server_event = ServerEvent::PresenceChanged(snapshot);
                        if let Ok(json) = serde_json::to_string(&server_event) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => {
                        presence_rx = None;
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    if let Some(snapshot) = presence.player_disconnected(game_id, player_id) {
        presence.broadcast(game_id, snapshot);
    }
}
