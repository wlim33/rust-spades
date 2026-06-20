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

use std::time::Duration;

use super::dto::{ErrorResponse, PresenceSnapshot, ServerEvent, WsQuery};
use super::handlers::games::seat_matches_identity;
use super::presence::PresenceTracker;
use super::{AppState, parse_uuid_or_short_id};

/// How often the server pings an idle game socket to confirm the peer is
/// still there. When it isn't our turn (or the game is over) no events
/// flow, so a peer whose TCP connection has silently died (laptop sleep,
/// dropped mobile link) never trips the send path and would linger as a
/// ghost "connected" presence holding a broadcast slot. The detection
/// window is one-to-two intervals.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// What a heartbeat tick should do, given whether the previous ping was
/// answered.
#[derive(Debug, PartialEq, Eq)]
enum Beat {
    /// Peer answered (or this is the first tick); send a fresh ping.
    Ping,
    /// The previous ping went unanswered for a full interval — treat the
    /// peer as dead and close the socket.
    Drop,
}

/// Tracks WebSocket peer liveness across heartbeat ticks. A tick that
/// finds the previous ping still unanswered means the peer is gone.
struct Heartbeat {
    awaiting_pong: bool,
}

impl Heartbeat {
    fn new() -> Self {
        Self {
            awaiting_pong: false,
        }
    }

    /// Called on each heartbeat tick.
    fn on_tick(&mut self) -> Beat {
        if self.awaiting_pong {
            // A full interval elapsed with the last ping unanswered.
            Beat::Drop
        } else {
            self.awaiting_pong = true;
            Beat::Ping
        }
    }

    /// Any frame from the peer (pong, ping, text, …) is proof of life.
    fn on_peer_activity(&mut self) {
        self.awaiting_pong = false;
    }
}

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

    let mut heartbeat = Heartbeat::new();
    let mut heartbeat_interval = tokio::time::interval(HEARTBEAT_INTERVAL);
    // The first tick fires immediately; let it pass so we don't ping the
    // instant the socket opens.
    heartbeat_interval.tick().await;

    loop {
        tokio::select! {
            _ = heartbeat_interval.tick() => {
                match heartbeat.on_tick() {
                    Beat::Ping => {
                        if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                            break;
                        }
                    }
                    Beat::Drop => {
                        // Peer never answered the previous ping; close the
                        // socket so its presence is released instead of
                        // lingering as a ghost.
                        let _ = socket.send(Message::Close(None)).await;
                        break;
                    }
                }
            }
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
                    Some(Ok(message)) => {
                        // Any frame from the peer proves the connection is
                        // alive, resetting the heartbeat's pending ping.
                        heartbeat.on_peer_activity();
                        match message {
                            Message::Ping(data) => {
                                if socket.send(Message::Pong(data)).await.is_err() {
                                    break;
                                }
                            }
                            Message::Close(_) => break,
                            _ => {}
                        }
                    }
                    Some(Err(_)) | None => break,
                }
            }
        }
    }

    if let Some(snapshot) = presence.player_disconnected(game_id, player_id) {
        presence.broadcast(game_id, snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_first_tick_pings() {
        // Nothing outstanding yet, so the first tick just sends a ping.
        let mut hb = Heartbeat::new();
        assert_eq!(hb.on_tick(), Beat::Ping);
    }

    #[test]
    fn heartbeat_drops_when_ping_goes_unanswered() {
        // First tick pings; if the next tick arrives with no peer activity
        // in between, the ping was never answered — the peer is dead.
        let mut hb = Heartbeat::new();
        assert_eq!(hb.on_tick(), Beat::Ping);
        assert_eq!(hb.on_tick(), Beat::Drop);
    }

    #[test]
    fn heartbeat_peer_activity_keeps_socket_alive() {
        // A pong (or any frame) between ticks proves the peer is alive, so
        // the following tick pings again instead of dropping.
        let mut hb = Heartbeat::new();
        assert_eq!(hb.on_tick(), Beat::Ping);
        hb.on_peer_activity();
        assert_eq!(hb.on_tick(), Beat::Ping);
        hb.on_peer_activity();
        assert_eq!(hb.on_tick(), Beat::Ping);
    }
}
