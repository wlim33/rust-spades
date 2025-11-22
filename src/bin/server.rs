use axum::{
    extract::{Path, State as AxumState},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use spades::game_manager::{
    CreateGameResponse, GameManager, GameStateResponse, HandResponse,
};
use spades::{Card, GameTransition};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    game_manager: GameManager,
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

#[tokio::main]
async fn main() {
    // Initialize the game manager
    let game_manager = GameManager::new();
    let app_state = AppState { game_manager };

    // Build the router
    let app = Router::new()
        .route("/", get(root))
        .route("/games", post(create_game))
        .route("/games", get(list_games))
        .route("/games/:game_id", get(get_game_state))
        .route("/games/:game_id", delete(delete_game))
        .route("/games/:game_id/transition", post(make_transition))
        .route("/games/:game_id/players/:player_id/hand", get(get_hand))
        .layer(CorsLayer::permissive())
        .with_state(app_state);

    // Start the server
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
            "delete_game": "DELETE /games/:game_id"
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
    state.game_manager.remove_game(game_id).map(|_| StatusCode::NO_CONTENT).map_err(|e| {
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

use axum::routing::delete;
