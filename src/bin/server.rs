use axum::{
    extract::{Path, State as AxumState},
    http::StatusCode,
    response::{
        Json,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use spades::game_manager::{
    CreateGameResponse, GameManager, GameStateResponse, HandResponse,
};
use spades::matchmaking::{LobbyEvent, LobbySummary, MatchResult, Matchmaker, SeekSummary};
use spades::{Card, GameTransition};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    game_manager: GameManager,
    matchmaker: Matchmaker,
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
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateLobbyRequest {
    #[serde(default = "default_max_points")]
    max_points: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeleteLobbyRequest {
    creator_id: Uuid,
}

fn default_max_points() -> i32 {
    500
}

#[tokio::main]
async fn main() {
    let game_manager = GameManager::new();
    let matchmaker = Matchmaker::new(game_manager.clone());
    let app_state = AppState {
        game_manager,
        matchmaker,
    };

    let app = Router::new()
        .route("/", get(root))
        .route("/games", post(create_game))
        .route("/games", get(list_games))
        .route("/games/:game_id", get(get_game_state))
        .route("/games/:game_id", delete(delete_game))
        .route("/games/:game_id/transition", post(make_transition))
        .route("/games/:game_id/players/:player_id/hand", get(get_hand))
        .route("/matchmaking/seek", post(seek))
        .route("/matchmaking/seeks", get(list_seeks_handler))
        .route("/lobbies", post(create_lobby))
        .route("/lobbies", get(list_lobbies_handler))
        .route("/lobbies/:lobby_id/join", post(join_lobby_handler))
        .route("/lobbies/:lobby_id", delete(delete_lobby_handler))
        .layer(CorsLayer::permissive())
        .with_state(app_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("Spades server listening on {}", addr);
    println!("\nAvailable endpoints:");
    println!("  GET  /                                          - API info");
    println!("  POST /games                                     - Create a new game");
    println!("  GET  /games                                     - List all games");
    println!("  GET  /games/:game_id                            - Get game state");
    println!("  POST /games/:game_id/transition                 - Make a move");
    println!("  GET  /games/:game_id/players/:player_id/hand    - Get player's hand");
    println!("  DELETE /games/:game_id                          - Delete a game");
    println!("  POST /matchmaking/seek                          - Quick match (SSE)");
    println!("  GET  /matchmaking/seeks                         - List active seeks");
    println!("  POST /lobbies                                   - Create lobby (SSE)");
    println!("  GET  /lobbies                                   - List open lobbies");
    println!("  POST /lobbies/:lobby_id/join                    - Join lobby (SSE)");
    println!("  DELETE /lobbies/:lobby_id                       - Delete lobby");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
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
        .create_game(request.max_points)
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

// --- Matchmaking: Seek Endpoints ---

async fn seek(
    AxumState(state): AxumState<AppState>,
    Json(request): Json<SeekRequest>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.matchmaker.add_seek(request.max_points);

    let stream = async_stream::stream! {
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

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    )
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
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let (lobby_id, player_id, mut rx) = state.matchmaker.create_lobby(request.max_points);

    let stream = async_stream::stream! {
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
                Ok(LobbyEvent::LobbyUpdate { lobby_id: lid, players }) => {
                    yield Ok(Event::default()
                        .event("lobby_update")
                        .json_data(serde_json::json!({
                            "lobby_id": lid,
                            "players": players,
                        }))
                        .unwrap());
                }
                Ok(LobbyEvent::GameStart(result)) => {
                    let personalized = MatchResult {
                        game_id: result.game_id,
                        player_id,
                        player_ids: result.player_ids,
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

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    )
}

async fn join_lobby_handler(
    AxumState(state): AxumState<AppState>,
    Path(lobby_id): Path<Uuid>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<ErrorResponse>),
> {
    let (player_id, mut rx) = state.matchmaker.join_lobby(lobby_id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("{:?}", e),
            }),
        )
    })?;

    let stream = async_stream::stream! {
        yield Ok(Event::default()
            .event("lobby_update")
            .json_data(serde_json::json!({
                "lobby_id": lobby_id,
                "player_id": player_id,
            }))
            .unwrap());

        loop {
            match rx.recv().await {
                Ok(LobbyEvent::LobbyUpdate { lobby_id: lid, players }) => {
                    yield Ok(Event::default()
                        .event("lobby_update")
                        .json_data(serde_json::json!({
                            "lobby_id": lid,
                            "players": players,
                        }))
                        .unwrap());
                }
                Ok(LobbyEvent::GameStart(result)) => {
                    let personalized = MatchResult {
                        game_id: result.game_id,
                        player_id,
                        player_ids: result.player_ids,
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
