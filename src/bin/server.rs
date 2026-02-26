use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State as AxumState,
    },
    http::StatusCode,
    response::{
        Json,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{delete, get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use spades::game_manager::{
    CreateGameResponse, GameEvent, GameManager, GameStateResponse, HandResponse,
};
use spades::challenges::{
    uuid_to_short_id, ChallengeConfig, ChallengeError, ChallengeEvent, ChallengeManager,
    ChallengeStatus, ChallengeSummary, Seat,
};
use spades::matchmaking::{LobbyEvent, LobbySummary, MatchResult, Matchmaker, SeekSummary};
use spades::validation::validate_player_name;
use spades::{Card, GameTransition};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub game_manager: GameManager,
    pub matchmaker: Matchmaker,
    pub challenge_manager: ChallengeManager,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateGameRequest {
    max_points: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct TransitionRequest {
    #[serde(flatten)]
    transition: TransitionType,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TransitionType {
    Start,
    Bet { amount: i32 },
    Card { card: Card },
}

#[derive(Debug, Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SeekRequest {
    #[serde(default = "default_max_points")]
    max_points: i32,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateLobbyRequest {
    #[serde(default = "default_max_points")]
    max_points: i32,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JoinLobbyRequest {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SetNameRequest {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeleteLobbyRequest {
    creator_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
struct JoinChallengeRequest {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CancelChallengeRequest {
    creator_id: Uuid,
}

fn default_max_points() -> i32 {
    500
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/games", post(create_game))
        .route("/games", get(list_games))
        .route("/games/:game_id", get(get_game_state))
        .route("/games/:game_id", delete(delete_game))
        .route("/games/:game_id/transition", post(make_transition))
        .route("/games/:game_id/players/:player_id/hand", get(get_hand))
        .route("/games/:game_id/players/:player_id/name", put(set_player_name))
        .route("/games/:game_id/ws", get(game_ws))
        .route("/matchmaking/seek", post(seek))
        .route("/matchmaking/seeks", get(list_seeks_handler))
        .route("/lobbies", post(create_lobby))
        .route("/lobbies", get(list_lobbies_handler))
        .route("/lobbies/:lobby_id/join", post(join_lobby_handler))
        .route("/lobbies/:lobby_id", delete(delete_lobby_handler))
        .route("/challenges", post(create_challenge_handler))
        .route("/challenges", get(list_challenges_handler))
        .route("/challenges/:challenge_id", get(get_challenge_handler))
        .route("/challenges/:challenge_id/join/:seat", post(join_challenge_handler))
        .route("/challenges/:challenge_id", delete(cancel_challenge_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let db_path = std::env::args()
        .skip_while(|a| a != "--db")
        .nth(1)
        .or_else(|| std::env::var("DATABASE_URL").ok());

    let game_manager = match db_path {
        Some(ref path) => {
            println!("Using SQLite database: {}", path);
            GameManager::with_db(path).expect("Failed to open database")
        }
        None => {
            println!("Running in-memory mode (no --db or DATABASE_URL set)");
            GameManager::new()
        }
    };
    let matchmaker = Matchmaker::new(game_manager.clone());
    let challenge_manager = ChallengeManager::new(game_manager.clone());
    let app_state = AppState {
        game_manager,
        matchmaker,
        challenge_manager,
    };

    let app = build_router(app_state);

    let port: u16 = std::env::args()
        .skip_while(|a| a != "--port")
        .nth(1)
        .or_else(|| std::env::var("PORT").ok())
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    println!("Spades server listening on {}", local_addr);
    println!("\nAvailable endpoints:");
    println!("  GET  /                                          - API info");
    println!("  POST /games                                     - Create a new game");
    println!("  GET  /games                                     - List all games");
    println!("  GET  /games/:game_id                            - Get game state");
    println!("  POST /games/:game_id/transition                 - Make a move");
    println!("  GET  /games/:game_id/players/:player_id/hand    - Get player's hand");
    println!("  PUT  /games/:game_id/players/:player_id/name    - Set player name");
    println!("  GET  /games/:game_id/ws                         - Game state WebSocket");
    println!("  DELETE /games/:game_id                          - Delete a game");
    println!("  POST /matchmaking/seek                          - Quick match (SSE)");
    println!("  GET  /matchmaking/seeks                         - List active seeks");
    println!("  POST /lobbies                                   - Create lobby (SSE)");
    println!("  GET  /lobbies                                   - List open lobbies");
    println!("  POST /lobbies/:lobby_id/join                    - Join lobby (SSE)");
    println!("  DELETE /lobbies/:lobby_id                       - Delete lobby");
    println!("  POST /challenges                                - Create challenge (SSE)");
    println!("  GET  /challenges                                - List open challenges");
    println!("  GET  /challenges/:challenge_id                  - Get challenge status");
    println!("  POST /challenges/:id/join/:seat                 - Join challenge seat (SSE)");
    println!("  DELETE /challenges/:challenge_id                - Cancel challenge");

    axum::serve(listener, app).await.unwrap();
}

async fn root() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "Spades Game Server",
        "version": "1.0.0",
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
            "create_lobby": "POST /lobbies",
            "list_lobbies": "GET /lobbies",
            "join_lobby": "POST /lobbies/:lobby_id/join",
            "delete_lobby": "DELETE /lobbies/:lobby_id"
        }
    }))
}

async fn create_game(
    AxumState(state): AxumState<AppState>,
    Json(request): Json<CreateGameRequest>,
) -> Result<Json<CreateGameResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .game_manager
        .create_game(request.max_points, None)
        .map(Json)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("{:?}", e),
                }),
            )
        })
}

async fn list_games(
    AxumState(state): AxumState<AppState>,
) -> Result<Json<Vec<Uuid>>, (StatusCode, Json<ErrorResponse>)> {
    state.game_manager.list_games().map(Json).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("{:?}", e),
            }),
        )
    })
}

async fn get_game_state(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
) -> Result<Json<GameStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .game_manager
        .get_game_state(game_id)
        .map(Json)
        .map_err(|e| {
            let status = match e {
                spades::game_manager::GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("{:?}", e),
                }),
            )
        })
}

async fn delete_game(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    state
        .game_manager
        .remove_game(game_id)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| {
            let status = match e {
                spades::game_manager::GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("{:?}", e),
                }),
            )
        })
}

async fn make_transition(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
    Json(request): Json<TransitionRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let transition = match request.transition {
        TransitionType::Start => GameTransition::Start,
        TransitionType::Bet { amount } => GameTransition::Bet(amount),
        TransitionType::Card { card } => GameTransition::Card(card),
    };

    state
        .game_manager
        .make_transition(game_id, transition)
        .map(|result| {
            Json(serde_json::json!({
                "success": true,
                "result": format!("{:?}", result)
            }))
        })
        .map_err(|e| {
            let status = match e {
                spades::game_manager::GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_REQUEST,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("{:?}", e),
                }),
            )
        })
}

async fn get_hand(
    AxumState(state): AxumState<AppState>,
    Path((game_id, player_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<HandResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .game_manager
        .get_hand(game_id, player_id)
        .map(Json)
        .map_err(|e| {
            let status = match e {
                spades::game_manager::GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_REQUEST,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("{:?}", e),
                }),
            )
        })
}

async fn set_player_name(
    AxumState(state): AxumState<AppState>,
    Path((game_id, player_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<SetNameRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
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
                spades::game_manager::GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_REQUEST,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("{:?}", e),
                }),
            )
        })
}

// --- WebSocket: Game state push ---

#[derive(Debug, Deserialize)]
struct WsQuery {
    player_id: Option<Uuid>,
}

async fn game_ws(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl axum::response::IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    let initial_state = state.game_manager.get_game_state(game_id).map_err(|e| {
        let status = match e {
            spades::game_manager::GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(ErrorResponse { error: format!("{:?}", e) }))
    })?;

    let rx = state.game_manager.subscribe(game_id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { error: format!("{:?}", e) }),
        )
    })?;

    let player_id = query.player_id;
    Ok(ws.on_upgrade(move |socket| handle_game_ws(socket, initial_state, rx, player_id)))
}

async fn handle_game_ws(
    mut socket: WebSocket,
    initial_state: GameStateResponse,
    mut rx: broadcast::Receiver<GameEvent>,
    _player_id: Option<Uuid>,
) {
    let initial_event = GameEvent::StateChanged(initial_state);
    if let Ok(json) = serde_json::to_string(&initial_event) {
        if socket.send(Message::Text(json.into())).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(game_event) => {
                        if let Ok(json) = serde_json::to_string(&game_event) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
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
}

// --- Matchmaking: Drop Guards ---

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

struct LobbyGuard {
    matchmaker: Matchmaker,
    lobby_id: Uuid,
    player_id: Uuid,
    game_started: bool,
}

impl Drop for LobbyGuard {
    fn drop(&mut self) {
        if !self.game_started {
            self.matchmaker.leave_lobby(self.lobby_id, self.player_id);
        }
    }
}

// --- Matchmaking: Seek Endpoints ---

async fn seek(
    AxumState(state): AxumState<AppState>,
    Json(request): Json<SeekRequest>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
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
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
        })?),
        None => None,
    };

    let (player_id, rx) = state.matchmaker.add_seek(request.max_points, validated_name);

    let stream = async_stream::stream! {
        let mut guard = SeekGuard {
            matchmaker: state.matchmaker.clone(),
            player_id,
            matched: false,
        };

        // Send initial queue status
        let seeks = state.matchmaker.list_seeks();
        let position = seeks.iter()
            .find(|s| s.max_points == request.max_points)
            .map(|s| s.waiting)
            .unwrap_or(0);

        yield Ok(Event::default()
            .event("queue_status")
            .json_data(serde_json::json!({
                "position": position,
                "waiting": position,
            }))
            .unwrap());

        // Wait for match result
        match rx.await {
            Ok(result) => {
                guard.matched = true;
                yield Ok(Event::default()
                    .event("game_start")
                    .json_data(&result)
                    .unwrap());
            }
            Err(_) => {
                // Channel dropped - seek was cancelled
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}

async fn list_seeks_handler(
    AxumState(state): AxumState<AppState>,
) -> Json<Vec<SeekSummary>> {
    Json(state.matchmaker.list_seeks())
}

// --- Matchmaking: Lobby Endpoints ---

async fn create_lobby(
    AxumState(state): AxumState<AppState>,
    Json(request): Json<CreateLobbyRequest>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
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
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
        })?),
        None => None,
    };

    let (lobby_id, player_id, mut rx) = state.matchmaker.create_lobby(request.max_points, validated_name);

    let stream = async_stream::stream! {
        let mut guard = LobbyGuard {
            matchmaker: state.matchmaker.clone(),
            lobby_id,
            player_id,
            game_started: false,
        };

        yield Ok(Event::default()
            .event("lobby_update")
            .json_data(serde_json::json!({
                "lobby_id": lobby_id,
                "player_id": player_id,
                "players": 1,
            }))
            .unwrap());

        loop {
            match rx.recv().await {
                Ok(LobbyEvent::LobbyUpdate { lobby_id: lid, players, player_names }) => {
                    yield Ok(Event::default()
                        .event("lobby_update")
                        .json_data(serde_json::json!({
                            "lobby_id": lid,
                            "players": players,
                            "player_names": player_names,
                        }))
                        .unwrap());
                }
                Ok(LobbyEvent::GameStart(result)) => {
                    guard.game_started = true;
                    let personalized = MatchResult {
                        game_id: result.game_id,
                        player_id,
                        player_ids: result.player_ids,
                        player_names: result.player_names.clone(),
                    };
                    yield Ok(Event::default()
                        .event("game_start")
                        .json_data(&personalized)
                        .unwrap());
                    break;
                }
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

async fn join_lobby_handler(
    AxumState(state): AxumState<AppState>,
    Path(lobby_id): Path<Uuid>,
    body: Option<Json<JoinLobbyRequest>>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<ErrorResponse>),
> {
    let validated_name = match body.and_then(|b| b.0.name) {
        Some(raw) => Some(validate_player_name(&raw).map_err(|e| {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
        })?),
        None => None,
    };

    let (player_id, mut rx) = state.matchmaker.join_lobby(lobby_id, validated_name).map_err(|e| {
        let status = match &e {
            spades::matchmaking::MatchmakingError::LobbyFull => StatusCode::CONFLICT,
            _ => StatusCode::NOT_FOUND,
        };
        (
            status,
            Json(ErrorResponse {
                error: format!("{:?}", e),
            }),
        )
    })?;

    let matchmaker = state.matchmaker.clone();
    let stream = async_stream::stream! {
        let mut guard = LobbyGuard {
            matchmaker,
            lobby_id,
            player_id,
            game_started: false,
        };

        yield Ok(Event::default()
            .event("lobby_update")
            .json_data(serde_json::json!({
                "lobby_id": lobby_id,
                "player_id": player_id,
            }))
            .unwrap());

        loop {
            match rx.recv().await {
                Ok(LobbyEvent::LobbyUpdate { lobby_id: lid, players, player_names }) => {
                    yield Ok(Event::default()
                        .event("lobby_update")
                        .json_data(serde_json::json!({
                            "lobby_id": lid,
                            "players": players,
                            "player_names": player_names,
                        }))
                        .unwrap());
                }
                Ok(LobbyEvent::GameStart(result)) => {
                    guard.game_started = true;
                    let personalized = MatchResult {
                        game_id: result.game_id,
                        player_id,
                        player_ids: result.player_ids,
                        player_names: result.player_names.clone(),
                    };
                    yield Ok(Event::default()
                        .event("game_start")
                        .json_data(&personalized)
                        .unwrap());
                    break;
                }
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

async fn list_lobbies_handler(
    AxumState(state): AxumState<AppState>,
) -> Json<Vec<LobbySummary>> {
    Json(state.matchmaker.list_lobbies())
}

async fn delete_lobby_handler(
    AxumState(state): AxumState<AppState>,
    Path(lobby_id): Path<Uuid>,
    Json(request): Json<DeleteLobbyRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    state
        .matchmaker
        .delete_lobby(lobby_id, request.creator_id)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("{:?}", e),
                }),
            )
        })
}

// --- Challenges: Drop Guard ---

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
            self.challenge_manager.vacate_seat(self.challenge_id, self.seat, self.player_id);
        }
    }
}

// --- Challenges: Endpoints ---

async fn create_challenge_handler(
    AxumState(state): AxumState<AppState>,
    Json(mut config): Json<ChallengeConfig>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
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

    let creator_seat = config.creator_seat;
    let (challenge_id, creator_player_id, mut rx) = state
        .challenge_manager
        .create_challenge(config)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("{:?}", e),
                }),
            )
        })?;

    let status = state.challenge_manager.get_status(challenge_id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("{:?}", e),
            }),
        )
    })?;

    let challenge_manager = state.challenge_manager.clone();
    let stream = async_stream::stream! {
        // Set up guard for creator if they took a seat
        let mut _guard = creator_seat.map(|seat| ChallengeGuard {
            challenge_manager: challenge_manager.clone(),
            challenge_id,
            seat,
            player_id: creator_player_id.unwrap_or(Uuid::nil()),
            game_started: false,
        });

        // Send initial challenge_created event
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
                    let personalized = MatchResult {
                        game_id: result.game_id,
                        player_id: creator_player_id.unwrap_or(Uuid::nil()),
                        player_ids: result.player_ids,
                        player_names: result.player_names.clone(),
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

async fn list_challenges_handler(
    AxumState(state): AxumState<AppState>,
) -> Json<Vec<ChallengeSummary>> {
    Json(state.challenge_manager.list_challenges())
}

async fn get_challenge_handler(
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
                    error: format!("{:?}", e),
                }),
            )
        })
}

async fn join_challenge_handler(
    AxumState(state): AxumState<AppState>,
    Path((challenge_id, seat_str)): Path<(Uuid, String)>,
    body: Option<Json<JoinChallengeRequest>>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<ErrorResponse>),
> {
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

    let (player_id, mut rx) = state
        .challenge_manager
        .join_challenge(challenge_id, seat, validated_name)
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
                    error: format!("{:?}", e),
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
                    let personalized = MatchResult {
                        game_id: result.game_id,
                        player_id,
                        player_ids: result.player_ids,
                        player_names: result.player_names.clone(),
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

async fn cancel_challenge_handler(
    AxumState(state): AxumState<AppState>,
    Path(challenge_id): Path<Uuid>,
    Json(request): Json<CancelChallengeRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    state
        .challenge_manager
        .cancel_challenge(challenge_id, request.creator_id)
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
                    error: format!("{:?}", e),
                }),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;
    use spades::game_manager::CreateGameResponse;

    fn test_app() -> TestServer {
        let game_manager = GameManager::new();
        let matchmaker = Matchmaker::new(game_manager.clone());
        let challenge_manager = ChallengeManager::new(game_manager.clone());
        let state = AppState {
            game_manager,
            matchmaker,
            challenge_manager,
        };
        TestServer::new(build_router(state)).unwrap()
    }

    #[tokio::test]
    async fn test_root_returns_200() {
        let server = test_app();
        let response = server.get("/").await;
        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["name"], "Spades Game Server");
    }

    #[tokio::test]
    async fn test_create_game() {
        let server = test_app();
        let response = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await;
        response.assert_status(StatusCode::OK);
        let body: CreateGameResponse = response.json();
        assert_eq!(body.player_ids.len(), 4);
    }

    #[tokio::test]
    async fn test_list_games_empty() {
        let server = test_app();
        let response = server.get("/games").await;
        response.assert_status_ok();
        let body: Vec<Uuid> = response.json();
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn test_list_games_after_create() {
        let server = test_app();
        server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await;
        let response = server.get("/games").await;
        let body: Vec<Uuid> = response.json();
        assert_eq!(body.len(), 1);
    }

    #[tokio::test]
    async fn test_get_game_state() {
        let server = test_app();
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        let response = server.get(&format!("/games/{}", create_resp.game_id)).await;
        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["state"], "NotStarted");
    }

    #[tokio::test]
    async fn test_get_game_state_not_found() {
        let server = test_app();
        let response = server.get(&format!("/games/{}", Uuid::new_v4())).await;
        response.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_game() {
        let server = test_app();
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        let response = server.delete(&format!("/games/{}", create_resp.game_id)).await;
        response.assert_status(StatusCode::NO_CONTENT);

        // Verify it's gone
        let list: Vec<Uuid> = server.get("/games").await.json();
        assert_eq!(list.len(), 0);
    }

    #[tokio::test]
    async fn test_delete_game_not_found() {
        let server = test_app();
        let response = server.delete(&format!("/games/{}", Uuid::new_v4())).await;
        response.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_start_game_transition() {
        let server = test_app();
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        let response = server
            .post(&format!("/games/{}/transition", create_resp.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await;
        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["success"], true);
    }

    #[tokio::test]
    async fn test_bet_transition() {
        let server = test_app();
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        // Start the game
        server
            .post(&format!("/games/{}/transition", create_resp.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await;

        // Place a bet
        let response = server
            .post(&format!("/games/{}/transition", create_resp.game_id))
            .json(&serde_json::json!({"type": "bet", "amount": 3}))
            .await;
        response.assert_status_ok();
    }

    #[tokio::test]
    async fn test_invalid_transition() {
        let server = test_app();
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        // Try to bet without starting — should fail
        let response = server
            .post(&format!("/games/{}/transition", create_resp.game_id))
            .json(&serde_json::json!({"type": "bet", "amount": 3}))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_transition_game_not_found() {
        let server = test_app();
        let response = server
            .post(&format!("/games/{}/transition", Uuid::new_v4()))
            .json(&serde_json::json!({"type": "start"}))
            .await;
        response.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_hand() {
        let server = test_app();
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        // Start game first
        server
            .post(&format!("/games/{}/transition", create_resp.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await;

        let response = server
            .get(&format!(
                "/games/{}/players/{}/hand",
                create_resp.game_id, create_resp.player_ids[0]
            ))
            .await;
        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["cards"].as_array().unwrap().len(), 13);
    }

    #[tokio::test]
    async fn test_get_hand_invalid_player() {
        let server = test_app();
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        server
            .post(&format!("/games/{}/transition", create_resp.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await;

        let response = server
            .get(&format!(
                "/games/{}/players/{}/hand",
                create_resp.game_id,
                Uuid::new_v4()
            ))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_hand_game_not_found() {
        let server = test_app();
        let response = server
            .get(&format!(
                "/games/{}/players/{}/hand",
                Uuid::new_v4(),
                Uuid::new_v4()
            ))
            .await;
        response.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_set_player_name() {
        let server = test_app();
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        let response = server
            .put(&format!(
                "/games/{}/players/{}/name",
                create_resp.game_id, create_resp.player_ids[0]
            ))
            .json(&serde_json::json!({"name": "Alice"}))
            .await;
        response.assert_status(StatusCode::NO_CONTENT);

        // Verify it persisted
        let state: serde_json::Value = server
            .get(&format!("/games/{}", create_resp.game_id))
            .await
            .json();
        assert_eq!(state["player_names"][0]["name"], "Alice");
    }

    #[tokio::test]
    async fn test_set_player_name_invalid() {
        let server = test_app();
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        let response = server
            .put(&format!(
                "/games/{}/players/{}/name",
                create_resp.game_id, create_resp.player_ids[0]
            ))
            .json(&serde_json::json!({"name": "fuck"}))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_clear_player_name() {
        let server = test_app();
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        // Set name
        server
            .put(&format!(
                "/games/{}/players/{}/name",
                create_resp.game_id, create_resp.player_ids[0]
            ))
            .json(&serde_json::json!({"name": "Alice"}))
            .await;

        // Clear name
        let response = server
            .put(&format!(
                "/games/{}/players/{}/name",
                create_resp.game_id, create_resp.player_ids[0]
            ))
            .json(&serde_json::json!({}))
            .await;
        response.assert_status(StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_list_seeks() {
        let server = test_app();
        let response = server.get("/matchmaking/seeks").await;
        response.assert_status_ok();
        let body: Vec<serde_json::Value> = response.json();
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn test_list_lobbies() {
        let server = test_app();
        let response = server.get("/lobbies").await;
        response.assert_status_ok();
        let body: Vec<serde_json::Value> = response.json();
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn test_delete_lobby_not_found() {
        let server = test_app();
        let response = server
            .delete(&format!("/lobbies/{}", Uuid::new_v4()))
            .json(&serde_json::json!({"creator_id": Uuid::new_v4()}))
            .await;
        response.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_challenges() {
        let server = test_app();
        let response = server.get("/challenges").await;
        response.assert_status_ok();
        let body: Vec<serde_json::Value> = response.json();
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn test_get_challenge_not_found() {
        let server = test_app();
        let response = server
            .get(&format!("/challenges/{}", Uuid::new_v4()))
            .await;
        response.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_cancel_challenge_not_found() {
        let server = test_app();
        let response = server
            .delete(&format!("/challenges/{}", Uuid::new_v4()))
            .json(&serde_json::json!({"creator_id": Uuid::new_v4()}))
            .await;
        response.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_full_game_flow_bet() {
        let server = test_app();

        // Create game
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        // Start
        server
            .post(&format!("/games/{}/transition", create_resp.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await
            .assert_status_ok();

        // Place 4 bets
        for _ in 0..3 {
            server
                .post(&format!("/games/{}/transition", create_resp.game_id))
                .json(&serde_json::json!({"type": "bet", "amount": 3}))
                .await
                .assert_status_ok();
        }

        let response = server
            .post(&format!("/games/{}/transition", create_resp.game_id))
            .json(&serde_json::json!({"type": "bet", "amount": 3}))
            .await;
        response.assert_status_ok();

        // Game should now be in Trick state
        let state: serde_json::Value = server
            .get(&format!("/games/{}", create_resp.game_id))
            .await
            .json();
        // State should exist (serialized as { "Trick": 0 } or similar)
        assert!(state.get("state").is_some());
    }
}
