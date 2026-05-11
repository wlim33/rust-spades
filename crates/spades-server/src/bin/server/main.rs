#![allow(
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::large_enum_variant,
    clippy::too_many_arguments,
)]

mod dto;
mod handlers;
mod presence;
mod ws;

use handlers::challenges::{
    cancel_challenge_handler, create_challenge_handler, get_challenge_by_short_id_handler,
    get_challenge_handler, join_challenge_handler, list_challenges_handler,
};
use handlers::games::{
    create_game, delete_game, get_game_by_player_url, get_game_by_short_id_handler,
    get_game_state, get_hand, get_presence, list_games, make_transition, root, set_player_name,
};
use handlers::matchmaking::{list_seeks_handler, queue_sizes_handler, seek};
use handlers::players::{get_player, set_display_name};
use presence::PresenceTracker;
use ws::game_ws;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use spades_server::game_manager::GameManager;
use spades_server::challenges::ChallengeManager;
use spades_server::matchmaking::Matchmaker;
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore as SessionSqliteStore;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub game_manager: GameManager,
    pub matchmaker: Matchmaker,
    pub challenge_manager: ChallengeManager,
    pub auth: spades_server::auth::AuthState,
    presence: PresenceTracker,
}

impl axum::extract::FromRef<AppState> for spades_server::auth::AuthState {
    fn from_ref(state: &AppState) -> Self {
        state.auth.clone()
    }
}

pub const SESSION_USER_KEY: &str = "user";

pub fn parse_uuid_or_short_id(s: &str) -> Option<Uuid> {
    Uuid::parse_str(s).ok().or_else(|| spades::short_id_to_uuid(s))
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
        .get("/games", list_games)
        .get("/games/{game_id}", get_game_state)
        .post("/games/{game_id}/transition", make_transition)
        .get("/games/{game_id}/players/{player_id}/hand", get_hand)
        .get("/games/by-short-id/{short_id}", get_game_by_short_id_handler)
        .get("/games/by-player-url/{url_id}", get_game_by_player_url)
        .get("/games/{game_id}/presence", get_presence)
        // Matchmaking
        .get("/matchmaking/seeks", list_seeks_handler)
        .get("/matchmaking/queue-sizes", queue_sizes_handler)
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
        // Handlers with non-OaSchema extractors (Identity, Session, etc.)
        .route("/games", post(create_game))
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
        // Auth endpoints
        .route("/auth/register", post(handlers::auth::register))
        .route("/auth/login", post(handlers::auth::login))
        .route("/auth/logout", post(handlers::auth::logout))
        .route("/auth/me", get(handlers::auth::me))
        .route("/auth/verify-email", get(handlers::auth::verify_email))
        .route("/auth/password-reset/request", post(handlers::auth::password_reset_request))
        .route("/auth/password-reset/confirm", post(handlers::auth::password_reset_confirm))
        .route("/auth/oauth/{provider}/login", get(handlers::auth::oauth_login))
        .route("/auth/oauth/google/callback", get(handlers::auth::oauth_google_callback))
        .route("/auth/oauth/github/callback", get(handlers::auth::oauth_github_callback))
        .route("/auth/oauth/complete", post(handlers::auth::oauth_complete))
        // User profile endpoints (literal /users/me must come before the wildcard)
        .route("/users/me", axum::routing::patch(spades_server::handlers_users::patch_me))
        .route("/users/{username}", get(spades_server::handlers_users::get_profile))
        .route("/users/{username}/games", get(spades_server::handlers_users::get_profile_games))
        .with_state(state)
        .layer(RequestBodyLimitLayer::new(1024 * 1024))
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(CatchPanicLayer::new())
}

/// Build a CORS layer from a list of allowed origins.
/// `"*"` enables a permissive layer; an empty list returns `None` (no CORS layer applied).
fn build_cors_layer(origins: &[String]) -> Option<CorsLayer> {
    if origins.is_empty() {
        None
    } else if origins.iter().any(|s| s == "*") {
        Some(CorsLayer::permissive())
    } else {
        let mut layer = CorsLayer::new();
        for o in origins {
            if let Ok(hv) = o.parse::<axum::http::HeaderValue>() {
                layer = layer.allow_origin(hv);
            }
        }
        Some(layer)
    }
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

    let insecure_cookies = std::env::args().any(|a| a == "--insecure-cookies");

    let auth_store_path = db_path.clone().unwrap_or_else(|| ":memory:".to_string());
    let auth_store = std::sync::Arc::new(
        spades_server::sqlite_store::SqliteStore::open(&auth_store_path)
            .expect("Failed to open auth SqliteStore"),
    );

    let mailer: std::sync::Arc<dyn spades_server::auth::mailer::Mailer> =
        match spades_server::auth::mailer::SmtpConfig::from_env() {
            Some(cfg) => match spades_server::auth::mailer::SmtpMailer::new(cfg) {
                Ok(m) => {
                    println!("Mailer: SmtpMailer (SMTP_HOST set)");
                    std::sync::Arc::new(m)
                }
                Err(e) => {
                    eprintln!("SmtpMailer init failed ({e}); falling back to LogMailer");
                    std::sync::Arc::new(spades_server::auth::mailer::LogMailer::new())
                }
            },
            None => {
                println!("Mailer: LogMailer (no SMTP_* env vars)");
                std::sync::Arc::new(spades_server::auth::mailer::LogMailer::new())
            }
        };

    let oauth = std::sync::Arc::new(spades_server::auth::oauth::OauthState::from_env());
    if oauth.google.is_some() { println!("OAuth: Google enabled"); }
    if oauth.github.is_some() { println!("OAuth: GitHub enabled"); }

    let rate = std::sync::Arc::new(spades_server::auth::rate_limit::RateLimitState::new());

    let auth_state = spades_server::auth::AuthState {
        store: auth_store,
        mailer,
        oauth,
        rate,
        secure_cookies: !insecure_cookies,
    };

    {
        let store = auth_state.store.clone();
        tokio::spawn(async move {
            // Cleanup once at startup, then every hour.
            loop {
                if let Err(e) = store.cleanup_expired_tokens() {
                    eprintln!("cleanup_expired_tokens: {e}");
                }
                tokio::time::sleep(std::time::Duration::from_secs(60 * 60)).await;
            }
        });
    }

    let app_state = AppState {
        game_manager,
        matchmaker,
        challenge_manager,
        auth: auth_state,
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
        .with_name("spades_session")
        .with_secure(!insecure_cookies)
        .with_http_only(true)
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::days(30)));

    let mut cors_origins: Vec<String> = Vec::new();
    let args: Vec<String> = std::env::args().collect();
    for (i, a) in args.iter().enumerate() {
        if a == "--cors-allow-origin" {
            if let Some(v) = args.get(i + 1) {
                cors_origins.push(v.clone());
            }
        }
    }
    if let Ok(env_origins) = std::env::var("CORS_ALLOW_ORIGIN") {
        for o in env_origins.split(',') {
            let o = o.trim();
            if !o.is_empty() {
                cors_origins.push(o.to_string());
            }
        }
    }

    let mut app = build_router(app_state).layer(session_layer);
    if let Some(cors) = build_cors_layer(&cors_origins) {
        app = app.layer(cors);
        println!("CORS enabled for origins: {}", cors_origins.join(", "));
    } else {
        println!("CORS layer not configured (set --cors-allow-origin <origin> or CORS_ALLOW_ORIGIN env)");
    }

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
    println!("  GET  /matchmaking/queue-sizes                   - Queue sizes by config");
    println!("  POST /challenges                                - Create challenge (SSE)");
    println!("  GET  /challenges                                - List open challenges");
    println!("  GET  /challenges/by-short-id/:short_id          - Get challenge by short ID");
    println!("  GET  /challenges/:challenge_id                  - Get challenge status");
    println!("  POST /challenges/:id/join/:seat                 - Join challenge seat (SSE)");
    println!("  DELETE /challenges/:challenge_id                - Cancel challenge");
    println!("  GET  /player                                    - Get/create session identity");
    println!("  PUT  /player/name                               - Set display name");
    println!("  GET  /users/:username                           - Public user profile");
    println!("  GET  /users/:username/games                     - User's game history (paginated)");
    println!("  PATCH /users/me                                 - Update own email or password");

    if insecure_cookies {
        println!("WARNING: --insecure-cookies enabled. Session cookie lacks Secure flag. DO NOT use in production.");
    }

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::PresenceSnapshot;
    use axum::http::StatusCode;
    use axum_test::{TestServer, TestServerConfig};
    use spades_server::game_manager::{CreateGameResponse, GameStateResponse};
    use tower_sessions::MemoryStore;

    fn test_app() -> TestServer {
        use axum::extract::connect_info::MockConnectInfo;

        let game_manager = GameManager::new();
        let matchmaker = Matchmaker::new(game_manager.clone());
        let challenge_manager = ChallengeManager::new(game_manager.clone());

        let auth_store = std::sync::Arc::new(
            spades_server::sqlite_store::SqliteStore::open(":memory:").unwrap()
        );
        let auth_state = spades_server::auth::AuthState {
            store: auth_store,
            mailer: std::sync::Arc::new(spades_server::auth::mailer::LogMailer::new()),
            oauth: std::sync::Arc::new(spades_server::auth::oauth::OauthState::from_env()),
            rate: std::sync::Arc::new(spades_server::auth::rate_limit::RateLimitState::new()),
            secure_cookies: false,
        };

        let state = AppState {
            game_manager,
            matchmaker,
            challenge_manager,
            auth: auth_state,
            presence: PresenceTracker::new(),
        };

        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false);

        let app = build_router(state)
            .layer(session_layer)
            .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
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
    async fn test_catch_panic_layer_isolates_failures() {
        use axum::Router;
        use axum::routing::get;
        use tower_http::catch_panic::CatchPanicLayer;

        async fn healthy() -> &'static str { "ok" }
        async fn boom() -> &'static str { panic!("intentional test panic") }

        let app: Router = Router::new()
            .route("/healthy", get(healthy))
            .route("/boom", get(boom))
            .layer(CatchPanicLayer::new());
        let server = TestServer::new(app).unwrap();

        server.get("/healthy").await.assert_status_ok();
        let resp = server.get("/boom").await;
        assert_eq!(resp.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        server.get("/healthy").await.assert_status_ok();
    }

    #[tokio::test]
    async fn test_request_id_generated_when_absent() {
        let server = test_app();
        let resp = server.get("/").await;
        resp.assert_status_ok();
        let id = resp.header("x-request-id");
        let id_str = id.to_str().expect("x-request-id should be utf-8");
        Uuid::parse_str(id_str).expect("x-request-id should be a valid UUID");
    }

    #[tokio::test]
    async fn test_request_id_propagated_from_request() {
        let server = test_app();
        let custom = "test-request-id-12345";
        let resp = server.get("/").add_header("x-request-id", custom).await;
        resp.assert_status_ok();
        assert_eq!(resp.header("x-request-id").to_str().unwrap(), custom);
    }

    #[tokio::test]
    async fn test_body_size_limit_rejects_oversized() {
        use axum::Router;
        use axum::routing::post;
        use tower_http::limit::RequestBodyLimitLayer;

        async fn echo(_body: axum::body::Bytes) -> &'static str { "ok" }

        let app: Router = Router::new()
            .route("/echo", post(echo))
            .layer(RequestBodyLimitLayer::new(1024));
        let server = TestServer::new(app).unwrap();

        let small = server.post("/echo").bytes(vec![0u8; 512].into()).await;
        small.assert_status_ok();

        let too_big = server.post("/echo").bytes(vec![0u8; 4096].into()).await;
        assert_eq!(too_big.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_timeout_layer_returns_error_on_slow_handler() {
        use axum::Router;
        use axum::routing::get;
        use tower_http::timeout::TimeoutLayer;

        async fn slow() -> &'static str {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            "ok"
        }

        let app: Router = Router::new()
            .route("/slow", get(slow))
            .layer(TimeoutLayer::new(std::time::Duration::from_millis(50)));
        let server = TestServer::new(app).unwrap();

        let resp = server.get("/slow").await;
        assert_eq!(resp.status_code(), StatusCode::REQUEST_TIMEOUT);
    }

    #[test]
    fn server_event_resync_serializes_with_tag() {
        use crate::dto::ServerEvent;
        let event = ServerEvent::Resync { reason: "lagged 5".to_string() };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["event"], "resync");
        assert_eq!(parsed["reason"], "lagged 5");
    }

    #[test]
    fn server_event_state_changed_carries_seq() {
        use crate::dto::ServerEvent;
        use spades_server::game_manager::GameStateResponse;
        let state = GameStateResponse {
            game_id: Uuid::nil(),
            short_id: spades::uuid_to_short_id(Uuid::nil()),
            state: spades::State::NotStarted,
            team_a_score: None,
            team_b_score: None,
            team_a_bags: None,
            team_b_bags: None,
            current_player_id: None,
            player_names: [
                spades_server::game_manager::PlayerNameEntry { player_id: Uuid::nil(), name: None },
                spades_server::game_manager::PlayerNameEntry { player_id: Uuid::nil(), name: None },
                spades_server::game_manager::PlayerNameEntry { player_id: Uuid::nil(), name: None },
                spades_server::game_manager::PlayerNameEntry { player_id: Uuid::nil(), name: None },
            ],
            timer_config: None,
            player_clocks_ms: None,
            active_player_clock_ms: None,
            table_cards: None,
            player_bets: None,
            player_tricks_won: None,
            last_trick_winner_id: None,
            last_completed_trick: None,
        };
        let event = ServerEvent::StateChanged { seq: 42, state };
        let parsed: serde_json::Value = serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(parsed["event"], "state_changed");
        assert_eq!(parsed["seq"], 42);
        // GameStateResponse is flattened — its fields appear at the top level.
        assert_eq!(parsed["state"], "NotStarted");
    }

    #[tokio::test]
    async fn lagged_subscriber_observes_lagged_error() {
        // Drives more broadcasts than the per-game buffer holds (64) without
        // consuming them, then verifies tokio's broadcast surfaces Lagged.
        // This is what the WS handler now translates into a `Resync` close
        // frame; the WS plumbing itself is too involved to unit-test here, so
        // we cover the broadcast layer plus the wire-format Resync separately.
        use spades_server::game_manager::{GameManager, GameEvent};
        use spades::GameTransition;
        use tokio::sync::broadcast;

        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        let (mut rx, _) = manager.subscribe(response.game_id).unwrap();

        manager.make_transition(response.game_id, GameTransition::Start).unwrap();
        for i in 0..80 {
            manager
                .set_player_name(response.game_id, response.player_ids[0], Some(format!("p{i}")))
                .unwrap();
        }

        let mut saw_lagged = false;
        for _ in 0..200 {
            match rx.try_recv() {
                Ok(GameEvent::StateChanged { .. }) | Ok(GameEvent::GameAborted { .. }) => continue,
                Err(broadcast::error::TryRecvError::Lagged(_)) => { saw_lagged = true; break; }
                Err(_) => break,
            }
        }
        assert!(saw_lagged, "expected Lagged after overflowing the broadcast buffer");
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
    async fn test_queue_sizes() {
        let server = test_app();
        let response = server.get("/matchmaking/queue-sizes").await;
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
        assert!(!body["paths"].as_object().unwrap().is_empty());
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
