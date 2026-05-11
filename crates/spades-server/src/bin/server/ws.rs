use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State as AxumState,
    },
    http::StatusCode,
    response::Json,
};
use spades_server::game_manager::{GameEvent, GameStateResponse};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::dto::{ErrorResponse, PresenceSnapshot, ServerEvent, WsQuery};
use super::presence::PresenceTracker;
use super::{parse_uuid_or_short_id, AppState};

pub async fn game_ws(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl axum::response::IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    let initial_state = state.game_manager.get_game_state(game_id).map_err(|e| {
        let status = match e {
            spades_server::game_manager::GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(ErrorResponse { error: format!("{}", e) }))
    })?;

    let rx = state.game_manager.subscribe(game_id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { error: format!("{}", e) }),
        )
    })?;

    let player_ids: Vec<Uuid> = initial_state.player_names.iter().map(|pn| pn.player_id).collect();
    state.presence.ensure_game(game_id, &player_ids);

    let presence_rx = state.presence.subscribe(game_id);
    let initial_presence = state.presence.get_snapshot(game_id);

    let player_id = query.player_id.as_deref().and_then(parse_uuid_or_short_id);
    let presence = state.presence.clone();
    Ok(ws.on_upgrade(move |socket| {
        handle_game_ws(socket, initial_state, rx, player_id, game_id, presence, presence_rx, initial_presence)
    }))
}

async fn handle_game_ws(
    mut socket: WebSocket,
    initial_state: GameStateResponse,
    mut rx: broadcast::Receiver<GameEvent>,
    player_id: Option<Uuid>,
    game_id: Uuid,
    presence: PresenceTracker,
    presence_rx: Option<broadcast::Receiver<PresenceSnapshot>>,
    initial_presence: Option<PresenceSnapshot>,
) {
    let initial_event = ServerEvent::StateChanged(initial_state);
    if let Ok(json) = serde_json::to_string(&initial_event) {
        if socket.send(Message::Text(json.into())).await.is_err() {
            return;
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

    if let Some(pid) = player_id {
        if let Some(snapshot) = presence.player_connected(game_id, pid) {
            presence.broadcast(game_id, snapshot);
        }
    }

    let mut presence_rx = presence_rx;

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(game_event) => {
                        let server_event = match game_event {
                            GameEvent::StateChanged(state) => ServerEvent::StateChanged(state),
                            GameEvent::GameAborted { game_id, reason } => ServerEvent::GameAborted { game_id, reason },
                        };
                        if let Ok(json) = serde_json::to_string(&server_event) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
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

    if let Some(pid) = player_id {
        if let Some(snapshot) = presence.player_disconnected(game_id, pid) {
            presence.broadcast(game_id, snapshot);
        }
    }
}
