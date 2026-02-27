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
use oasgen::{oasgen, OaSchema};
use serde::{Deserialize, Serialize};
use spades::game_manager::{
    CreateGameResponse, GameEvent, GameManager, GameStateResponse, HandResponse,
};
use spades::challenges::{
    ChallengeConfig, ChallengeError, ChallengeEvent, ChallengeManager,
    ChallengeStatus, ChallengeSummary, Seat,
};
use spades::matchmaking::{MatchResult, Matchmaker, SeekEvent, SeekSummary};
use spades::validation::validate_player_name;
use spades::{Card, GameTransition, TimerConfig};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tower_sessions::{Expiry, Session, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore as SessionSqliteStore;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub game_manager: GameManager,
    pub matchmaker: Matchmaker,
    pub challenge_manager: ChallengeManager,
    presence: PresenceTracker,
}

const SESSION_USER_KEY: &str = "user";

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UserSession {
    user_id: Uuid,
    display_name: Option<String>,
}

#[derive(Debug, Serialize, oasgen::OaSchema)]
struct SessionPlayerResponse {
    user_id: Uuid,
    display_name: Option<String>,
}

#[derive(Debug, Deserialize, oasgen::OaSchema)]
struct SetDisplayNameRequest {
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, oasgen::OaSchema)]
struct CreateGameRequest {
    #[serde(default = "default_max_points")]
    max_points: i32,
    timer_config: Option<TimerConfig>,
    num_humans: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize, oasgen::OaSchema)]
struct TransitionRequest {
    #[serde(flatten)]
    transition: TransitionType,
}

#[derive(Debug, Serialize, Deserialize, oasgen::OaSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TransitionType {
    Start,
    Bet { amount: i32 },
    Card { card: Card },
}

#[derive(Debug, Serialize, OaSchema)]
struct TransitionResponse {
    success: bool,
    result: String,
}

fn parse_uuid_or_short_id(s: &str) -> Option<Uuid> {
    Uuid::parse_str(s).ok().or_else(|| spades::short_id_to_uuid(s))
}

#[derive(Debug, Serialize, Deserialize, oasgen::OaSchema)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Serialize, oasgen::OaSchema)]
struct PlayerUrlResponse {
    game_id: Uuid,
    player_id: Uuid,
    player_short_id: String,
    game: GameStateResponse,
    hand: HandResponse,
}

#[derive(Debug, Serialize, Deserialize, oasgen::OaSchema)]
struct SeekRequest {
    #[serde(default = "default_max_points")]
    max_points: i32,
    timer_config: TimerConfig,
    #[serde(default)]
    name: Option<String>,
}


#[derive(Debug, Serialize, Deserialize, oasgen::OaSchema)]
struct SetNameRequest {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, oasgen::OaSchema)]
struct JoinChallengeRequest {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, oasgen::OaSchema)]
struct CancelChallengeRequest {
    creator_id: Uuid,
}

fn default_max_points() -> i32 {
    500
}

pub fn build_router(state: AppState) -> Router {
    let mut server = oasgen::Server::axum();
    server.openapi.info.title = "Spades Game Server".to_string();
    server.openapi.info.version = env!("CARGO_PKG_VERSION").to_string();
    server.openapi.info.description = Some(
        "4-player Spades card game server with matchmaking, challenges, and real-time updates."
            .to_string(),
    );

    let server = server
        // Game endpoints
        .post("/games", create_game)
        .get("/games", list_games)
        .get("/games/{game_id}", get_game_state)
        .post("/games/{game_id}/transition", make_transition)
        .get("/games/{game_id}/players/{player_id}/hand", get_hand)
        .get("/games/by-short-id/{short_id}", get_game_by_short_id_handler)
        .get("/games/by-player-url/{url_id}", get_game_by_player_url)
        .get("/games/{game_id}/presence", get_presence)
        // Matchmaking
        .get("/matchmaking/seeks", list_seeks_handler)
        // Challenges
        .get("/challenges", list_challenges_handler)
        .get("/challenges/{challenge_id}", get_challenge_handler)
        .get("/challenges/by-short-id/{short_id}", get_challenge_by_short_id_handler)
        // Spec endpoints
        .route_json_spec("/openapi.json")
        .route_yaml_spec("/openapi.yaml")
        .swagger_ui("/docs/")
        .freeze();

    Router::new()
        .merge(server.into_router())
        // Root
        .route("/", get(root))
        // Handlers returning StatusCode (not JSON body — oasgen needs Json responses)
        .route("/games/{game_id}", delete(delete_game))
        .route("/games/{game_id}/players/{player_id}/name", put(set_player_name))
        .route("/challenges/{challenge_id}", delete(cancel_challenge_handler))
        // Session-based (oasgen can't handle Session extractor)
        .route("/player", get(get_player))
        .route("/player/name", put(set_display_name))
        // SSE endpoints
        .route("/matchmaking/seek", post(seek))
        .route("/challenges", post(create_challenge_handler))
        .route("/challenges/{challenge_id}/join/{seat}", post(join_challenge_handler))
        // WebSocket
        .route("/games/{game_id}/ws", get(game_ws))
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
        presence: PresenceTracker::new(),
    };

    // Session store setup
    let session_db_url = match db_path {
        Some(ref path) => format!("sqlite:{}?mode=rwc", path),
        None => "sqlite::memory:".to_string(),
    };
    let session_pool = tower_sessions_sqlx_store::sqlx::SqlitePool::connect(&session_db_url)
        .await
        .expect("Failed to connect session SQLite pool");
    let session_store = SessionSqliteStore::new(session_pool);
    session_store.migrate().await.expect("Failed to migrate session store");

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(time::Duration::days(30)));

    let app = build_router(app_state).layer(session_layer);

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
    println!("  GET  /games/:game_id/presence                   - Player presence");
    println!("  DELETE /games/:game_id                          - Delete a game");
    println!("  POST /matchmaking/seek                          - Quick match (SSE)");
    println!("  GET  /matchmaking/seeks                         - List active seeks");
    println!("  POST /challenges                                - Create challenge (SSE)");
    println!("  GET  /challenges                                - List open challenges");
    println!("  GET  /challenges/by-short-id/:short_id          - Get challenge by short ID");
    println!("  GET  /challenges/:challenge_id                  - Get challenge status");
    println!("  POST /challenges/:id/join/:seat                 - Join challenge seat (SSE)");
    println!("  DELETE /challenges/:challenge_id                - Cancel challenge");
    println!("  GET  /player                                    - Get/create session identity");
    println!("  PUT  /player/name                               - Set display name");

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
        }
    }))
}

#[oasgen]
async fn create_game(
    AxumState(state): AxumState<AppState>,
    Json(request): Json<CreateGameRequest>,
) -> Result<Json<CreateGameResponse>, (StatusCode, Json<ErrorResponse>)> {
    let map_err = |e: spades::game_manager::GameManagerError| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("{:?}", e),
            }),
        )
    };

    match request.num_humans {
        None | Some(4) => {
            // Human-only game
            let response = state
                .game_manager
                .create_game(request.max_points, request.timer_config)
                .map_err(map_err)?;
            state.presence.ensure_game(response.game_id, &response.player_ids);
            Ok(Json(response))
        }
        Some(1) | Some(2) => {
            // AI game
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

            // Init presence and mark AI players as connected
            state.presence.ensure_game(game_id, &response.player_ids);
            for i in 0..4 {
                if !human_seats.contains(&i) {
                    state.presence.player_connected(game_id, response.player_ids[i]);
                }
            }

            // Auto-start the game
            state
                .game_manager
                .make_transition(game_id, spades::GameTransition::Start)
                .map_err(map_err)?;

            // Play through initial AI turns
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

#[oasgen]
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

#[oasgen]
async fn get_game_by_short_id_handler(
    AxumState(state): AxumState<AppState>,
    Path(short_id): Path<String>,
) -> Result<Json<GameStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    let game_id = spades::short_id_to_uuid(&short_id).ok_or((
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
async fn get_game_by_player_url(
    AxumState(state): AxumState<AppState>,
    Path(url_id): Path<String>,
) -> Result<Json<PlayerUrlResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (game_id, player_id) = spades::decode_player_url(&url_id).ok_or((
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
        player_short_id: spades::uuid_to_short_id(player_id),
        game,
        hand,
    }))
}

async fn delete_game(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let result = state
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
        })?;
    state.presence.remove_game(game_id);
    Ok(result)
}

#[oasgen]
async fn make_transition(
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
                spades::game_manager::GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_REQUEST,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("{:?}", e),
                }),
            )
        })?;

    // Auto-play AI turns if this game has AI players
    let _ = state.game_manager.play_ai_turns(game_id);

    Ok(Json(TransitionResponse {
        success: true,
        result: format!("{:?}", result),
    }))
}

#[oasgen]
async fn get_hand(
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

// --- Presence: REST endpoint ---

#[oasgen]
async fn get_presence(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
) -> Result<Json<PresenceSnapshot>, (StatusCode, Json<ErrorResponse>)> {
    // Lazy init: if tracker doesn't know this game, look up player_ids from game state
    if state.presence.get_snapshot(game_id).is_none() {
        let game_state = state.game_manager.get_game_state(game_id).map_err(|e| {
            let status = match e {
                spades::game_manager::GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse { error: format!("{:?}", e) }))
        })?;
        let player_ids: Vec<Uuid> = game_state.player_names.iter().map(|pn| pn.player_id).collect();
        state.presence.ensure_game(game_id, &player_ids);
    }
    state.presence.get_snapshot(game_id).map(Json).ok_or((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse { error: "Game not found".to_string() }),
    ))
}

// --- WebSocket: Game state push ---

#[derive(Debug, Deserialize)]
struct WsQuery {
    player_id: Option<String>,
}

// --- Presence Tracking ---

#[derive(Debug, Clone, Serialize, Deserialize, oasgen::OaSchema)]
struct PlayerPresenceEntry {
    player_id: Uuid,
    connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, oasgen::OaSchema)]
struct PresenceSnapshot {
    game_id: Uuid,
    players: Vec<PlayerPresenceEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum ServerEvent {
    StateChanged(GameStateResponse),
    GameAborted { game_id: Uuid, reason: String },
    PresenceChanged(PresenceSnapshot),
}

/// Per-game connection counts: game_id -> (player_id -> connection_count)
#[derive(Clone)]
struct PresenceTracker {
    /// game_id -> (player_id -> active connection count)
    connections: Arc<RwLock<HashMap<Uuid, HashMap<Uuid, usize>>>>,
    /// game_id -> broadcast sender for presence snapshots
    broadcasters: Arc<RwLock<HashMap<Uuid, broadcast::Sender<PresenceSnapshot>>>>,
}

impl PresenceTracker {
    fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            broadcasters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Idempotent init for a game. Creates entry + broadcaster if missing.
    fn ensure_game(&self, game_id: Uuid, player_ids: &[Uuid]) {
        let mut conns = self.connections.write().unwrap();
        conns.entry(game_id).or_insert_with(|| {
            player_ids.iter().map(|&pid| (pid, 0usize)).collect()
        });
        let mut bcast = self.broadcasters.write().unwrap();
        bcast.entry(game_id).or_insert_with(|| broadcast::channel(16).0);
    }

    /// Increment connection count. Returns snapshot if player is in the game.
    fn player_connected(&self, game_id: Uuid, player_id: Uuid) -> Option<PresenceSnapshot> {
        let mut conns = self.connections.write().unwrap();
        let game = conns.get_mut(&game_id)?;
        // Only track known players (ignore spectators)
        let count = game.get_mut(&player_id)?;
        *count += 1;
        Some(self.build_snapshot_from(game_id, game))
    }

    /// Decrement connection count (saturating). Returns snapshot if player is in the game.
    fn player_disconnected(&self, game_id: Uuid, player_id: Uuid) -> Option<PresenceSnapshot> {
        let mut conns = self.connections.write().unwrap();
        let game = conns.get_mut(&game_id)?;
        let count = game.get_mut(&player_id)?;
        *count = count.saturating_sub(1);
        Some(self.build_snapshot_from(game_id, game))
    }

    /// Read-only current state.
    fn get_snapshot(&self, game_id: Uuid) -> Option<PresenceSnapshot> {
        let conns = self.connections.read().unwrap();
        let game = conns.get(&game_id)?;
        Some(self.build_snapshot_from(game_id, game))
    }

    /// Subscribe to presence broadcasts for a game.
    fn subscribe(&self, game_id: Uuid) -> Option<broadcast::Receiver<PresenceSnapshot>> {
        let bcast = self.broadcasters.read().unwrap();
        bcast.get(&game_id).map(|tx| tx.subscribe())
    }

    /// Send snapshot to all subscribers.
    fn broadcast(&self, game_id: Uuid, snapshot: PresenceSnapshot) {
        let bcast = self.broadcasters.read().unwrap();
        if let Some(tx) = bcast.get(&game_id) {
            let _ = tx.send(snapshot);
        }
    }

    /// Clean up tracker state for a deleted game.
    fn remove_game(&self, game_id: Uuid) {
        self.connections.write().unwrap().remove(&game_id);
        self.broadcasters.write().unwrap().remove(&game_id);
    }

    fn build_snapshot_from(&self, game_id: Uuid, game: &HashMap<Uuid, usize>) -> PresenceSnapshot {
        PresenceSnapshot {
            game_id,
            players: game
                .iter()
                .map(|(&pid, &count)| PlayerPresenceEntry {
                    player_id: pid,
                    connected: count > 0,
                })
                .collect(),
        }
    }
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

    // Lazy init presence for games created via matchmaking/challenge
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
    // Send initial game state as ServerEvent
    let initial_event = ServerEvent::StateChanged(initial_state);
    if let Ok(json) = serde_json::to_string(&initial_event) {
        if socket.send(Message::Text(json.into())).await.is_err() {
            return;
        }
    }

    // Send initial presence snapshot
    if let Some(snapshot) = initial_presence {
        let event = ServerEvent::PresenceChanged(snapshot);
        if let Ok(json) = serde_json::to_string(&event) {
            if socket.send(Message::Text(json.into())).await.is_err() {
                return;
            }
        }
    }

    // Mark player connected and broadcast
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
                        // Presence channel closed, disable this arm
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

    // Mark player disconnected on exit
    if let Some(pid) = player_id {
        if let Some(snapshot) = presence.player_disconnected(game_id, pid) {
            presence.broadcast(game_id, snapshot);
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

    let (player_id, mut rx) = state.matchmaker.add_seek(request.max_points, request.timer_config, validated_name);

    let stream = async_stream::stream! {
        let mut guard = SeekGuard {
            matchmaker: state.matchmaker.clone(),
            player_id,
            matched: false,
        };

        // Listen for queue updates and match result
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
async fn list_seeks_handler(
    AxumState(state): AxumState<AppState>,
) -> Json<Vec<SeekSummary>> {
    Json(state.matchmaker.list_seeks())
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
async fn list_challenges_handler(
    AxumState(state): AxumState<AppState>,
) -> Json<Vec<ChallengeSummary>> {
    Json(state.challenge_manager.list_challenges())
}

#[oasgen]
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

#[oasgen]
async fn get_challenge_by_short_id_handler(
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
                error: format!("{:?}", e),
            }),
        )),
    }
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

// --- Session: Player Identity ---

async fn get_player(session: Session) -> Result<Json<SessionPlayerResponse>, StatusCode> {
    let user = match session.get::<UserSession>(SESSION_USER_KEY).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        Some(user) => user,
        None => {
            let user = UserSession {
                user_id: Uuid::new_v4(),
                display_name: None,
            };
            session.insert(SESSION_USER_KEY, user.clone()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            user
        }
    };
    Ok(Json(SessionPlayerResponse {
        user_id: user.user_id,
        display_name: user.display_name,
    }))
}

async fn set_display_name(
    session: Session,
    Json(request): Json<SetDisplayNameRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let mut user: UserSession = session
        .get::<UserSession>(SESSION_USER_KEY)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Session error".to_string() })))?
        .ok_or((StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "No session. Call GET /player first.".to_string() })))?;

    let validated_name = match request.name {
        Some(raw) => Some(validate_player_name(&raw).map_err(|e| {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
        })?),
        None => None,
    };

    user.display_name = validated_name;
    session.insert(SESSION_USER_KEY, user).await.map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Session error".to_string() }))
    })?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::{TestServer, TestServerConfig};
    use spades::game_manager::CreateGameResponse;
    use tower_sessions::MemoryStore;

    fn test_app() -> TestServer {
        let game_manager = GameManager::new();
        let matchmaker = Matchmaker::new(game_manager.clone());
        let challenge_manager = ChallengeManager::new(game_manager.clone());
        let state = AppState {
            game_manager,
            matchmaker,
            challenge_manager,
            presence: PresenceTracker::new(),
        };

        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false);

        let app = build_router(state).layer(session_layer);
        TestServer::new_with_config(
            app,
            TestServerConfig {
                save_cookies: true,
                ..Default::default()
            },
        )
        .unwrap()
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

    #[tokio::test]
    async fn test_get_challenge_by_short_id_not_found() {
        let server = test_app();
        let response = server.get("/challenges/by-short-id/abc123").await;
        response.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_card_transition() {
        let server = test_app();

        // Create and start game
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        server
            .post(&format!("/games/{}/transition", create_resp.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await
            .assert_status_ok();

        // Place 4 bets
        for _ in 0..4 {
            server
                .post(&format!("/games/{}/transition", create_resp.game_id))
                .json(&serde_json::json!({"type": "bet", "amount": 3}))
                .await
                .assert_status_ok();
        }

        // Get current player's hand and play first card
        let state: serde_json::Value = server
            .get(&format!("/games/{}", create_resp.game_id))
            .await
            .json();
        let current_pid = state["current_player_id"].as_str().unwrap();

        let hand: serde_json::Value = server
            .get(&format!(
                "/games/{}/players/{}/hand",
                create_resp.game_id, current_pid
            ))
            .await
            .json();
        let first_card = &hand["cards"][0];

        let response = server
            .post(&format!("/games/{}/transition", create_resp.game_id))
            .json(&serde_json::json!({
                "type": "card",
                "card": first_card
            }))
            .await;
        response.assert_status_ok();
    }

    #[tokio::test]
    async fn test_set_player_name_game_not_found() {
        let server = test_app();
        let response = server
            .put(&format!(
                "/games/{}/players/{}/name",
                Uuid::new_v4(),
                Uuid::new_v4()
            ))
            .json(&serde_json::json!({"name": "Alice"}))
            .await;
        response.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_set_player_name_invalid_player_id() {
        let server = test_app();
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        let response = server
            .put(&format!(
                "/games/{}/players/{}/name",
                create_resp.game_id,
                Uuid::new_v4()
            ))
            .json(&serde_json::json!({"name": "Alice"}))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_set_player_name_empty() {
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
            .json(&serde_json::json!({"name": ""}))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_ai_game_1_human() {
        let app = test_app();
        let response = app
            .post("/games")
            .json(&serde_json::json!({ "max_points": 200, "num_humans": 1 }))
            .await;
        response.assert_status_ok();
        let body: CreateGameResponse = response.json();
        assert_ne!(body.game_id, Uuid::nil());
    }

    #[tokio::test]
    async fn test_create_ai_game_2_humans() {
        let app = test_app();
        let response = app
            .post("/games")
            .json(&serde_json::json!({ "max_points": 200, "num_humans": 2 }))
            .await;
        response.assert_status_ok();
        let body: CreateGameResponse = response.json();
        assert_ne!(body.game_id, Uuid::nil());
    }

    #[tokio::test]
    async fn test_create_ai_game_invalid_num_humans() {
        let app = test_app();
        let response = app
            .post("/games")
            .json(&serde_json::json!({ "max_points": 200, "num_humans": 3 }))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);

        let response = app
            .post("/games")
            .json(&serde_json::json!({ "max_points": 200, "num_humans": 0 }))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ai_game_state_after_creation() {
        let app = test_app();
        let response = app
            .post("/games")
            .json(&serde_json::json!({ "max_points": 500, "num_humans": 1 }))
            .await;
        response.assert_status_ok();
        let body: CreateGameResponse = response.json();

        let state_response = app
            .get(&format!("/games/{}", body.game_id))
            .await;
        state_response.assert_status_ok();
        let state: GameStateResponse = state_response.json();
        // Human is player 0 — after auto-start and AI betting, it should be the human's turn
        assert_eq!(state.current_player_id, Some(body.player_ids[0]));
    }

    // --- Session tests ---

    #[tokio::test]
    async fn test_get_player_creates_session() {
        let server = test_app();
        let response = server.get("/player").await;
        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert!(body["user_id"].is_string());
        assert!(body["display_name"].is_null());
    }

    #[tokio::test]
    async fn test_get_player_returns_same_user_id() {
        let server = test_app();
        let first: serde_json::Value = server.get("/player").await.json();
        let second: serde_json::Value = server.get("/player").await.json();
        assert_eq!(first["user_id"], second["user_id"]);
    }

    #[tokio::test]
    async fn test_set_display_name() {
        let server = test_app();
        // Create session first
        server.get("/player").await;

        let response = server
            .put("/player/name")
            .json(&serde_json::json!({"name": "Alice"}))
            .await;
        response.assert_status(StatusCode::NO_CONTENT);

        // Verify via GET /player
        let body: serde_json::Value = server.get("/player").await.json();
        assert_eq!(body["display_name"], "Alice");
    }

    #[tokio::test]
    async fn test_set_display_name_invalid() {
        let server = test_app();
        server.get("/player").await;

        let response = server
            .put("/player/name")
            .json(&serde_json::json!({"name": ""}))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_set_display_name_clear() {
        let server = test_app();
        server.get("/player").await;

        // Set name
        server
            .put("/player/name")
            .json(&serde_json::json!({"name": "Alice"}))
            .await
            .assert_status(StatusCode::NO_CONTENT);

        // Clear name
        server
            .put("/player/name")
            .json(&serde_json::json!({"name": null}))
            .await
            .assert_status(StatusCode::NO_CONTENT);

        let body: serde_json::Value = server.get("/player").await.json();
        assert!(body["display_name"].is_null());
    }

    #[tokio::test]
    async fn test_set_display_name_no_session() {
        let server = test_app();
        // Don't call GET /player first
        let response = server
            .put("/player/name")
            .json(&serde_json::json!({"name": "Alice"}))
            .await;
        response.assert_status(StatusCode::UNAUTHORIZED);
    }

    // --- Presence Tracker unit tests ---

    #[test]
    fn test_presence_tracker_connect_disconnect() {
        let tracker = PresenceTracker::new();
        let game_id = Uuid::new_v4();
        let p1 = Uuid::new_v4();
        let p2 = Uuid::new_v4();

        tracker.ensure_game(game_id, &[p1, p2]);

        // Initially all disconnected
        let snap = tracker.get_snapshot(game_id).unwrap();
        assert!(snap.players.iter().all(|p| !p.connected));

        // Connect p1
        let snap = tracker.player_connected(game_id, p1).unwrap();
        let p1_entry = snap.players.iter().find(|p| p.player_id == p1).unwrap();
        assert!(p1_entry.connected);
        let p2_entry = snap.players.iter().find(|p| p.player_id == p2).unwrap();
        assert!(!p2_entry.connected);

        // Disconnect p1
        let snap = tracker.player_disconnected(game_id, p1).unwrap();
        let p1_entry = snap.players.iter().find(|p| p.player_id == p1).unwrap();
        assert!(!p1_entry.connected);
    }

    #[test]
    fn test_presence_multiple_connections() {
        let tracker = PresenceTracker::new();
        let game_id = Uuid::new_v4();
        let p1 = Uuid::new_v4();

        tracker.ensure_game(game_id, &[p1]);

        // Two connections
        tracker.player_connected(game_id, p1);
        let snap = tracker.player_connected(game_id, p1).unwrap();
        assert!(snap.players[0].connected);

        // Disconnect one — still connected
        let snap = tracker.player_disconnected(game_id, p1).unwrap();
        assert!(snap.players[0].connected);

        // Disconnect second — now disconnected
        let snap = tracker.player_disconnected(game_id, p1).unwrap();
        assert!(!snap.players[0].connected);
    }

    #[test]
    fn test_presence_spectator_ignored() {
        let tracker = PresenceTracker::new();
        let game_id = Uuid::new_v4();
        let p1 = Uuid::new_v4();
        let spectator = Uuid::new_v4();

        tracker.ensure_game(game_id, &[p1]);

        // Spectator connect returns None (not in game)
        assert!(tracker.player_connected(game_id, spectator).is_none());
        // Spectator disconnect returns None
        assert!(tracker.player_disconnected(game_id, spectator).is_none());

        // Original player unaffected
        let snap = tracker.get_snapshot(game_id).unwrap();
        assert!(!snap.players[0].connected);
    }

    #[test]
    fn test_presence_remove_game() {
        let tracker = PresenceTracker::new();
        let game_id = Uuid::new_v4();
        let p1 = Uuid::new_v4();

        tracker.ensure_game(game_id, &[p1]);
        assert!(tracker.get_snapshot(game_id).is_some());

        tracker.remove_game(game_id);
        assert!(tracker.get_snapshot(game_id).is_none());
        assert!(tracker.subscribe(game_id).is_none());
    }

    // --- Presence integration tests ---

    #[tokio::test]
    async fn test_get_presence_endpoint() {
        let server = test_app();
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        let response = server
            .get(&format!("/games/{}/presence", create_resp.game_id))
            .await;
        response.assert_status_ok();
        let snap: PresenceSnapshot = response.json();
        assert_eq!(snap.game_id, create_resp.game_id);
        assert_eq!(snap.players.len(), 4);
        // All players should be disconnected initially
        assert!(snap.players.iter().all(|p| !p.connected));
    }

    #[tokio::test]
    async fn test_get_presence_not_found() {
        let server = test_app();
        let response = server
            .get(&format!("/games/{}/presence", Uuid::new_v4()))
            .await;
        response.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_ai_game_presence() {
        let server = test_app();
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500, "num_humans": 1}))
            .await
            .json();

        let response = server
            .get(&format!("/games/{}/presence", create_resp.game_id))
            .await;
        response.assert_status_ok();
        let snap: PresenceSnapshot = response.json();
        assert_eq!(snap.players.len(), 4);
        // Player 0 is human (disconnected), players 1-3 are AI (connected)
        let human_pid = create_resp.player_ids[0];
        let human_entry = snap.players.iter().find(|p| p.player_id == human_pid).unwrap();
        assert!(!human_entry.connected);
        // All 3 AI players should be connected
        let ai_connected: Vec<_> = snap.players.iter()
            .filter(|p| p.player_id != human_pid && p.connected)
            .collect();
        assert_eq!(ai_connected.len(), 3);
    }

    // --- OpenAPI spec tests ---

    #[tokio::test]
    async fn test_openapi_json_endpoint() {
        let server = test_app();
        let response = server.get("/openapi.json").await;
        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["info"]["title"], "Spades Game Server");
        assert!(body["paths"].as_object().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn test_openapi_yaml_endpoint() {
        let server = test_app();
        let response = server.get("/openapi.yaml").await;
        response.assert_status_ok();
    }

    #[tokio::test]
    async fn test_swagger_ui_endpoint() {
        let server = test_app();
        let response = server.get("/docs/").await;
        response.assert_status_ok();
    }
}
