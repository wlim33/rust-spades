#![allow(
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::large_enum_variant,
    clippy::too_many_arguments
)]

mod dto;
mod handlers;
mod idempotency;
mod presence;
mod ws;

use handlers::challenges::{
    cancel_challenge_handler, create_challenge_handler, get_challenge_by_short_id_handler,
    get_challenge_handler, join_challenge_handler, list_challenges_handler,
};
use handlers::games::{
    create_game, delete_game, get_game_by_player_url, get_game_by_short_id_handler, get_game_state,
    get_hand, get_presence, get_replay, make_transition, post_chat, root, set_player_name,
};
use handlers::matchmaking::{list_seeks_handler, queue_sizes_handler, seek};
use handlers::players::{get_player, set_display_name};
use presence::PresenceTracker;
use ws::game_ws;

use axum::{
    Router,
    routing::{delete, get, post, put},
};
use spades_server::challenges::ChallengeManager;
use spades_server::game_manager::GameManager;
use spades_server::matchmaking::Matchmaker;
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore as SessionSqliteStore;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub game_manager: GameManager,
    pub matchmaker: Matchmaker,
    pub challenge_manager: ChallengeManager,
    pub auth: spades_server::auth::AuthState,
    presence: PresenceTracker,
    pub idempotency: std::sync::Arc<idempotency::IdempotencyCache>,
}

impl axum::extract::FromRef<AppState> for spades_server::auth::AuthState {
    fn from_ref(state: &AppState) -> Self {
        state.auth.clone()
    }
}

pub const SESSION_USER_KEY: &str = "user";

pub fn parse_uuid_or_short_id(s: &str) -> Option<Uuid> {
    Uuid::parse_str(s)
        .ok()
        .or_else(|| spades::short_id_to_uuid(s))
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
        .get("/games/{game_id}", get_game_state)
        .post("/games/{game_id}/transition", make_transition)
        .get("/games/{game_id}/players/{player_id}/hand", get_hand)
        .get(
            "/games/by-short-id/{short_id}",
            get_game_by_short_id_handler,
        )
        .get("/games/by-player-url/{url_id}", get_game_by_player_url)
        .get("/games/{game_id}/presence", get_presence)
        // Matchmaking
        .get("/matchmaking/seeks", list_seeks_handler)
        .get("/matchmaking/queue-sizes", queue_sizes_handler)
        // Challenges
        .get("/challenges", list_challenges_handler)
        .get("/challenges/{challenge_id}", get_challenge_handler)
        .get(
            "/challenges/by-short-id/{short_id}",
            get_challenge_by_short_id_handler,
        )
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
        .route(
            "/games/{game_id}/players/{player_id}/name",
            put(set_player_name),
        )
        // Replay returns text/plain (oasgen wants JSON responses).
        .route("/games/{game_id}/replay", get(get_replay))
        // Chat: POST sends a message; subscribers see it via WS.
        .route("/games/{game_id}/chat", post(post_chat))
        .route(
            "/challenges/{challenge_id}",
            delete(cancel_challenge_handler),
        )
        // Session-based (oasgen can't handle Session extractor)
        .route("/player", get(get_player))
        .route("/player/name", put(set_display_name))
        // SSE endpoints
        .route("/matchmaking/seek", post(seek))
        .route("/challenges", post(create_challenge_handler))
        .route(
            "/challenges/{challenge_id}/join/{seat}",
            post(join_challenge_handler),
        )
        // WebSocket
        .route("/games/{game_id}/ws", get(game_ws))
        // Auth endpoints
        .route("/auth/register", post(handlers::auth::register))
        .route("/auth/login", post(handlers::auth::login))
        .route("/auth/logout", post(handlers::auth::logout))
        .route(
            "/auth/tokens",
            post(spades_server::handlers_auth::create_token),
        )
        .route(
            "/auth/tokens",
            get(spades_server::handlers_auth::list_tokens),
        )
        .route(
            "/auth/tokens/{token_id}",
            axum::routing::delete(spades_server::handlers_auth::revoke_token),
        )
        .route("/auth/me", get(handlers::auth::me))
        .route("/auth/verify-email", get(handlers::auth::verify_email))
        .route(
            "/auth/password-reset/request",
            post(handlers::auth::password_reset_request),
        )
        .route(
            "/auth/password-reset/confirm",
            post(handlers::auth::password_reset_confirm),
        )
        .route(
            "/auth/oauth/{provider}/login",
            get(handlers::auth::oauth_login),
        )
        .route(
            "/auth/oauth/google/callback",
            get(handlers::auth::oauth_google_callback),
        )
        .route(
            "/auth/oauth/github/callback",
            get(handlers::auth::oauth_github_callback),
        )
        .route("/auth/oauth/complete", post(handlers::auth::oauth_complete))
        // User profile endpoints (literal /users/me must come before the wildcard)
        .route(
            "/users/me",
            axum::routing::patch(spades_server::handlers_users::patch_me),
        )
        .route(
            "/users/{username}",
            get(spades_server::handlers_users::get_profile),
        )
        .route(
            "/users/{username}/games",
            get(spades_server::handlers_users::get_profile_games),
        )
        .route(
            "/leaderboard",
            get(spades_server::handlers_leaderboard::get_leaderboard),
        )
        // Operational endpoints — outside the oasgen-managed schema.
        .route("/health", get(health))
        .route("/readyz", get(readyz))
        .with_state(state)
        .layer(RequestBodyLimitLayer::new(1024 * 1024))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(CatchPanicLayer::new())
}

/// Liveness probe — returns 200 while the process is alive enough to serve
/// HTTP. Does NOT check downstream dependencies; use `/readyz` for that.
async fn health() -> &'static str {
    "ok"
}

/// Derive the session-store SQLite URL from the game DB path.
///
/// The session store (sqlx) MUST NOT share the game DB file (rusqlite): a
/// single SQLite file opened by two libraries contends for the WAL lock when a
/// deploy swaps the container, which stalled startup long enough to fail the
/// healthcheck and abort the deploy. A sibling `sessions.sqlite` removes the
/// coupling entirely. In-memory / no-DB runs keep an in-memory session store.
fn session_db_url(db_path: Option<&str>) -> String {
    match db_path {
        None | Some(":memory:") => "sqlite::memory:".to_string(),
        Some(path) => {
            let sessions = std::path::Path::new(path).with_file_name("sessions.sqlite");
            format!("sqlite:{}?mode=rwc", sessions.display())
        }
    }
}

/// Readiness probe — returns 200 when the server is ready to accept traffic.
/// Currently only verifies the in-process state is reachable; a future
/// version should ping the SQLite store with a cheap query.
async fn readyz(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<&'static str, axum::http::StatusCode> {
    state
        .game_manager
        .list_games()
        .map(|_| "ok")
        .map_err(|_| axum::http::StatusCode::SERVICE_UNAVAILABLE)
}

/// Build a CORS layer from a list of allowed origins.
/// `"*"` enables a permissive layer; an empty list returns `None` (no CORS layer applied).
fn build_cors_layer(origins: &[String]) -> Option<CorsLayer> {
    if origins.is_empty() {
        None
    } else if origins.iter().any(|s| s == "*") {
        Some(CorsLayer::permissive())
    } else {
        // Exact-origin allowlist with credentials: the frontend sends the
        // session cookie (credentials: 'include'), which the CORS spec only
        // permits with a non-wildcard origin, allow-credentials: true, and
        // concrete (here: mirrored) methods/headers in the preflight answer.
        // Origins go in as one list — repeated allow_origin calls replace
        // rather than accumulate.
        let origin_values: Vec<axum::http::HeaderValue> =
            origins.iter().filter_map(|o| o.parse().ok()).collect();
        Some(
            CorsLayer::new()
                .allow_credentials(true)
                .allow_methods(tower_http::cors::AllowMethods::mirror_request())
                .allow_headers(tower_http::cors::AllowHeaders::mirror_request())
                .allow_origin(origin_values),
        )
    }
}

use clap::Parser;

/// spades-server command-line + environment configuration.
#[derive(Parser, Debug)]
#[command(
    version,
    about = "Spades game server (HTTP/WebSocket + matchmaking + challenges)"
)]
struct Args {
    /// Port to listen on.
    #[arg(long, env = "PORT", default_value_t = 3000)]
    port: u16,

    /// SQLite database path; omit for in-memory (state not persisted).
    #[arg(long, env = "DATABASE_URL")]
    db: Option<String>,

    /// Drop the Secure flag on the session cookie (dev only, over http).
    #[arg(long)]
    insecure_cookies: bool,

    /// Allowed CORS origin(s); repeatable, or comma-separated via CORS_ALLOW_ORIGIN.
    #[arg(
        long = "cors-allow-origin",
        env = "CORS_ALLOW_ORIGIN",
        value_delimiter = ','
    )]
    cors_allow_origin: Vec<String>,
}

#[cfg(test)]
mod config_tests {
    use super::Args;
    use clap::Parser;

    #[test]
    fn defaults() {
        let a = Args::try_parse_from(["spades-server"]).unwrap();
        assert_eq!(a.port, 3000);
        assert!(a.db.is_none());
        assert!(!a.insecure_cookies);
        assert!(a.cors_allow_origin.is_empty());
    }

    #[test]
    fn explicit_flags() {
        let a = Args::try_parse_from([
            "spades-server",
            "--port",
            "4000",
            "--db",
            "x.sqlite",
            "--insecure-cookies",
            "--cors-allow-origin",
            "http://a",
            "--cors-allow-origin",
            "http://b",
        ])
        .unwrap();
        assert_eq!(a.port, 4000);
        assert_eq!(a.db.as_deref(), Some("x.sqlite"));
        assert!(a.insecure_cookies);
        assert_eq!(a.cors_allow_origin, ["http://a", "http://b"]);
    }

    #[test]
    fn comma_separated_cors() {
        let a = Args::try_parse_from(["spades-server", "--cors-allow-origin", "http://a,http://b"])
            .unwrap();
        assert_eq!(a.cors_allow_origin, ["http://a", "http://b"]);
    }

    #[test]
    fn invalid_port_errors() {
        assert!(Args::try_parse_from(["spades-server", "--port", "notaport"]).is_err());
    }

    #[test]
    fn unknown_flag_errors() {
        assert!(Args::try_parse_from(["spades-server", "--nope"]).is_err());
    }
}

#[tokio::main]
async fn main() {
    init_tracing();
    validate_startup_config();

    #[cfg(feature = "insecure-fast-hash")]
    warn!(
        "built with `insecure-fast-hash`: password hashes use throwaway argon2 params — test builds only, do NOT deploy"
    );

    let args = Args::parse();
    let db_path = args.db;

    let game_manager = match db_path {
        Some(ref path) => {
            info!(path = %path, "using SQLite database");
            GameManager::with_db(path).expect("Failed to open database")
        }
        None => {
            warn!(
                "running in-memory mode (no --db or DATABASE_URL set) — state will not persist across restarts"
            );
            GameManager::new()
        }
    };
    let matchmaker = Matchmaker::new(game_manager.clone());
    let challenge_manager = ChallengeManager::new(game_manager.clone());

    let insecure_cookies = args.insecure_cookies;

    let auth_store_path = db_path.clone().unwrap_or_else(|| ":memory:".to_string());
    let auth_store = std::sync::Arc::new(
        spades_server::sqlite_store::SqliteStore::open(&auth_store_path)
            .expect("Failed to open auth SqliteStore"),
    );

    let backing_mailer: std::sync::Arc<dyn spades_server::auth::mailer::Mailer> =
        match spades_server::auth::mailer::SmtpConfig::from_env() {
            Some(cfg) => match spades_server::auth::mailer::SmtpMailer::new(cfg) {
                Ok(m) => {
                    info!("mailer: SmtpMailer (SMTP_HOST set)");
                    std::sync::Arc::new(m)
                }
                Err(e) => {
                    warn!(error = %e, "SmtpMailer init failed; falling back to LogMailer");
                    std::sync::Arc::new(spades_server::auth::mailer::LogMailer::new())
                }
            },
            None => {
                info!("mailer: LogMailer (no SMTP_* env vars)");
                std::sync::Arc::new(spades_server::auth::mailer::LogMailer::new())
            }
        };

    // Wrap with a queue so handlers don't await SMTP latency. The background
    // drain task lives for the runtime's lifetime; on shutdown, any
    // not-yet-delivered emails are lost — both verify-email and password-
    // reset flows are recoverable by re-requesting.
    let mailer: std::sync::Arc<dyn spades_server::auth::mailer::Mailer> = std::sync::Arc::new(
        spades_server::auth::mailer::MailerQueue::new(backing_mailer),
    );

    let oauth = std::sync::Arc::new(spades_server::auth::oauth::OauthState::from_env());
    if oauth.google.is_some() {
        info!("OAuth: Google enabled");
    }
    if oauth.github.is_some() {
        info!("OAuth: GitHub enabled");
    }

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
                    warn!(error = %e, "cleanup_expired_tokens failed");
                }
                tokio::time::sleep(std::time::Duration::from_secs(60 * 60)).await;
            }
        });
    }

    {
        // Sweep expired OAuth `csrf` and `pending` state every 5 min so a
        // drive-by attacker hitting `/auth/<provider>/login` can't grow the
        // in-memory maps without bound. csrf entries typically live ~10 min
        // (the OAuth client's `state` lifetime); a 5-min sweep catches most
        // within one window.
        let oauth = auth_state.oauth.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5 * 60)).await;
                let removed = oauth.sweep_expired();
                if removed > 0 {
                    tracing::debug!(removed, "swept expired OAuth state entries");
                }
            }
        });
    }

    let idempotency_cache = std::sync::Arc::new(idempotency::IdempotencyCache::new());

    {
        // Sweep stale idempotency entries every TTL window so a flood of
        // unique keys can't grow the cache without bound.
        let cache = idempotency_cache.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(idempotency::TTL).await;
                let removed = cache.sweep_expired();
                if removed > 0 {
                    tracing::debug!(removed, "swept expired idempotency entries");
                }
            }
        });
    }

    {
        // Drop completed games from in-memory state after they've been
        // terminal for 1 h; subsequent reads rehydrate from SQLite on
        // demand. Bounds memory growth on long-lived deployments.
        let manager = game_manager.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5 * 60)).await;
                let evicted = manager
                    .sweep_completed_games(std::time::Duration::from_secs(60 * 60))
                    .await;
                if evicted > 0 {
                    tracing::debug!(evicted, "evicted completed games from memory");
                }
            }
        });
    }

    let app_state = AppState {
        game_manager,
        matchmaker,
        challenge_manager,
        auth: auth_state,
        presence: PresenceTracker::new(),
        idempotency: idempotency_cache,
    };

    // Session store setup. Keep it on its OWN SQLite file (sibling of the game
    // DB): sharing one file between sqlx (here) and rusqlite (the game store)
    // makes the container contend for the WAL lock during a deploy swap, which
    // failed the healthcheck and took the backend down.
    let session_url = session_db_url(db_path.as_deref());
    let session_pool = tower_sessions_sqlx_store::sqlx::SqlitePool::connect(&session_url)
        .await
        .expect("Failed to connect session SQLite pool");
    let session_store = SessionSqliteStore::new(session_pool);
    session_store
        .migrate()
        .await
        .expect("Failed to migrate session store");

    let session_layer = SessionManagerLayer::new(session_store)
        .with_name("spades_session")
        .with_secure(!insecure_cookies)
        .with_http_only(true)
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::days(30)));

    let cors_origins = args.cors_allow_origin;

    let mut app = build_router(app_state).layer(session_layer);
    if let Some(cors) = build_cors_layer(&cors_origins) {
        app = app.layer(cors);
        info!(origins = %cors_origins.join(", "), "CORS enabled");
    } else {
        info!(
            "CORS layer not configured (set --cors-allow-origin <origin> or CORS_ALLOW_ORIGIN env)"
        );
    }

    let port = args.port;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    info!(
        addr = %local_addr,
        docs = %format!("http://{local_addr}/docs/"),
        "spades server listening; OpenAPI schema at /docs/",
    );

    if insecure_cookies {
        warn!(
            "--insecure-cookies enabled — session cookie lacks Secure flag; DO NOT use in production"
        );
    }

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .unwrap();
}

/// Surface partial config at startup with loud warnings so operators don't
/// have to discover at first user attempt that e.g. `SMTP_HOST` is set but
/// `SMTP_USER` is missing (which silently falls back to `LogMailer`). Each
/// case is independently warned and the server still starts — easier to
/// deploy than fatal errors when env vars get juggled.
fn validate_startup_config() {
    for w in collect_config_warnings(|name| std::env::var(name).ok()) {
        warn!("{w}");
    }
}

/// Pure-logic core of `validate_startup_config`. Reads env vars via the
/// caller-supplied `get` closure so tests can drive it with a mock map
/// without touching process-global `std::env`.
fn collect_config_warnings(get: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let mut warnings = Vec::new();

    // SMTP: setting HOST without the credential / sender vars silently
    // disables outbound email; password-reset and email-verify links will
    // never be delivered to the user.
    if get("SMTP_HOST").is_some() {
        for v in ["SMTP_USER", "SMTP_PASS", "SMTP_FROM"] {
            if get(v).is_none() {
                warnings.push(format!(
                    "SMTP_HOST is set but {v} is missing — outbound email will silently fall back to LogMailer"
                ));
            }
        }
    }

    // OAuth: setting the client ID without the secret (or vice versa)
    // silently disables that provider, leaving the login button broken.
    for (id_var, secret_var) in [
        ("GOOGLE_OAUTH_CLIENT_ID", "GOOGLE_OAUTH_CLIENT_SECRET"),
        ("GITHUB_OAUTH_CLIENT_ID", "GITHUB_OAUTH_CLIENT_SECRET"),
    ] {
        let id_set = get(id_var).is_some();
        let sec_set = get(secret_var).is_some();
        if id_set ^ sec_set {
            let missing = if id_set { secret_var } else { id_var };
            warnings.push(format!(
                "partial OAuth config — {missing} missing; provider will be disabled"
            ));
        }
    }

    // OAUTH_REDIRECT_BASE_URL must look like a URL; the OAuth flow builds
    // callback URLs by appending paths to it, so a malformed value produces
    // confusing runtime errors. Don't pull in a URL parser for this single
    // check — a cheap scheme-prefix gate catches the common typo.
    if let Some(url) = get("OAUTH_REDIRECT_BASE_URL") {
        let looks_like_url = url.starts_with("http://") || url.starts_with("https://");
        if !looks_like_url {
            warnings.push(format!(
                "OAUTH_REDIRECT_BASE_URL ({url}) does not start with http:// or https:// — OAuth callback URLs will be malformed"
            ));
        }
    }

    warnings
}

/// Initialize the tracing subscriber. Log level defaults to `info`; override
/// with `RUST_LOG` (e.g. `RUST_LOG=spades_server=debug,tower_http=info`).
fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // try_init so tests / parallel runs that already initialized a subscriber
    // don't panic on double-init.
    let _ = fmt().with_env_filter(filter).try_init();
}

/// Wait for either Ctrl+C or SIGTERM (Unix). On non-Unix only Ctrl+C is
/// observed. In-flight HTTP requests complete and the listener stops
/// accepting new connections. WebSocket subscribers are dropped when the
/// runtime shuts down — game state is already persisted on every transition
/// when `--db` is set, so the loss is bounded to mid-transition windows.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("Ctrl+C received; shutting down"),
        _ = terminate => info!("SIGTERM received; shutting down"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::PresenceSnapshot;
    use axum::http::StatusCode;
    use axum_test::{TestServer, TestServerConfig, Transport};
    use spades_server::game_manager::{CreateGameResponse, GameStateResponse};
    use tower_sessions::MemoryStore;

    fn test_app() -> TestServer {
        test_app_with_store().0
    }

    /// Like `test_app`, but also hands back the auth store so a test can
    /// inspect or rewrite seat ownership directly (e.g. to simulate a game
    /// whose seats belong to several distinct identities, which the HTTP
    /// surface only produces via SSE-driven challenge/matchmaking flows).
    fn test_app_with_store() -> (
        TestServer,
        std::sync::Arc<spades_server::sqlite_store::SqliteStore>,
    ) {
        use axum::extract::connect_info::MockConnectInfo;

        let game_manager = GameManager::new();
        let matchmaker = Matchmaker::new(game_manager.clone());
        let challenge_manager = ChallengeManager::new(game_manager.clone());

        let auth_store = std::sync::Arc::new(
            spades_server::sqlite_store::SqliteStore::open(":memory:").unwrap(),
        );
        let auth_state = spades_server::auth::AuthState {
            store: auth_store.clone(),
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
            idempotency: std::sync::Arc::new(idempotency::IdempotencyCache::new()),
        };

        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store).with_secure(false);

        let app = build_router(state)
            .layer(session_layer)
            .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
        let server = TestServer::new_with_config(
            app,
            TestServerConfig {
                save_cookies: true,
                transport: Some(Transport::HttpRandomPort),
                ..Default::default()
            },
        )
        .unwrap();
        (server, auth_store)
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
    async fn bot_account_bearer_token_authenticates_as_owner() {
        // Register a user via session, issue an API token, then drop the
        // session cookie and use Bearer auth instead. The bot must be
        // recognised as the registered user (POST /games succeeds with
        // identity-derived rate limits keyed on the user_id).
        let mut server = test_app();
        let register = server
            .post("/auth/register")
            .json(&serde_json::json!({
                "username": "alice",
                "email": "alice@example.com",
                "password": "supersecret-passphrase",
            }))
            .await;
        assert_eq!(register.status_code(), StatusCode::CREATED);
        let create_token = server
            .post("/auth/tokens")
            .json(&serde_json::json!({"name": "my bot"}))
            .await;
        assert_eq!(create_token.status_code(), StatusCode::CREATED);
        let body: serde_json::Value = create_token.json();
        let plaintext = body["token"].as_str().expect("token plaintext").to_string();
        let token_id = body["id"].as_str().unwrap().to_string();

        // Switch to "bot client" mode: no cookies, Bearer header set.
        server.clear_cookies();
        let resp = server
            .post("/games")
            .add_header("authorization", format!("Bearer {plaintext}"))
            .json(&serde_json::json!({"max_points": 500}))
            .await;
        assert_eq!(resp.status_code(), StatusCode::OK);

        // Revoke the token — subsequent Bearer auth must now fail with 401.
        // Switch back to the registered user's session for the revoke call.
        // The simplest way: log in again to re-establish a cookie session.
        server
            .post("/auth/login")
            .json(&serde_json::json!({
                "login": "alice",
                "password": "supersecret-passphrase",
            }))
            .await
            .assert_status_ok();
        let revoke = server.delete(&format!("/auth/tokens/{token_id}")).await;
        assert_eq!(revoke.status_code(), StatusCode::NO_CONTENT);

        // Now Bearer with the same (revoked) token: 401.
        server.clear_cookies();
        let resp = server
            .post("/games")
            .add_header("authorization", format!("Bearer {plaintext}"))
            .json(&serde_json::json!({"max_points": 500}))
            .await;
        assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn chat_owner_can_send_message() {
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        let resp = server
            .post(&format!("/games/{}/chat", create.game_id))
            .json(&serde_json::json!({
                "player_id": create.player_ids[0],
                "content": "gg",
            }))
            .await;
        assert_eq!(resp.status_code(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn chat_non_owner_is_forbidden() {
        let mut server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        // Flip to a fresh anon session and try to send chat as that seat.
        server.clear_cookies();
        let resp = server
            .post(&format!("/games/{}/chat", create.game_id))
            .json(&serde_json::json!({
                "player_id": create.player_ids[0],
                "content": "hi",
            }))
            .await;
        assert_eq!(resp.status_code(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn chat_rejects_empty_and_oversized_content() {
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        let path = format!("/games/{}/chat", create.game_id);

        let empty = server
            .post(&path)
            .json(&serde_json::json!({"player_id": create.player_ids[0], "content": ""}))
            .await;
        assert_eq!(empty.status_code(), StatusCode::BAD_REQUEST);

        let oversized: String = "a".repeat(501);
        let too_big = server
            .post(&path)
            .json(&serde_json::json!({"player_id": create.player_ids[0], "content": oversized}))
            .await;
        assert_eq!(too_big.status_code(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn chat_rejects_inappropriate_content() {
        // `rustrict::CensorStr::is_inappropriate()` flags severe profanity.
        // The handler must return 400 with a content-filter-specific error
        // before even checking seat ownership — the message never reaches
        // the broadcast.
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        let resp = server
            .post(&format!("/games/{}/chat", create.game_id))
            .json(&serde_json::json!({
                "player_id": create.player_ids[0],
                "content": "fuck you cunt",
            }))
            .await;
        assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = resp.json();
        assert!(
            body["error"]
                .as_str()
                .unwrap_or("")
                .contains("content filter"),
            "expected content-filter error, got: {:?}",
            body["error"],
        );
    }

    #[tokio::test]
    async fn chat_returns_404_for_unknown_player_id() {
        // Player UUID is well-formed but not seated in any game → 400 from
        // the seat lookup, never reaches the broadcast.
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        let resp = server
            .post(&format!("/games/{}/chat", create.game_id))
            .json(&serde_json::json!({
                "player_id": Uuid::new_v4(),
                "content": "ready",
            }))
            .await;
        assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn idempotency_key_replays_cached_error_outcome() {
        // The cache stores both success and failure outcomes — so a retry
        // after a 4xx returns the same 4xx, never re-running the transition
        // against drifted state. Bet-before-start fails with BAD_REQUEST;
        // a retry with the same idempotency-key must match status + body.
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        let first = server
            .post(&format!("/games/{}/transition", create.game_id))
            .add_header("idempotency-key", "retry-err")
            .json(&serde_json::json!({"type": "bet", "amount": 3}))
            .await;
        assert_eq!(first.status_code(), StatusCode::BAD_REQUEST);
        let first_body: serde_json::Value = first.json();

        let second = server
            .post(&format!("/games/{}/transition", create.game_id))
            .add_header("idempotency-key", "retry-err")
            .json(&serde_json::json!({"type": "bet", "amount": 3}))
            .await;
        assert_eq!(second.status_code(), StatusCode::BAD_REQUEST);
        let second_body: serde_json::Value = second.json();
        assert_eq!(first_body, second_body, "cached error replays identically");
    }

    #[tokio::test]
    async fn replay_returns_404_for_missing_game() {
        let server = test_app();
        let resp = server
            .get(&format!("/games/{}/replay", Uuid::new_v4()))
            .await;
        assert_eq!(resp.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn replay_returns_403_for_in_progress_game() {
        // Hand transcripts mid-game would leak hidden hands; the handler
        // gates on terminal state.
        let server = test_app();
        let create = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await;
        create.assert_status_ok();
        let game: CreateGameResponse = create.json();

        // NotStarted — definitely in progress.
        let resp = server.get(&format!("/games/{}/replay", game.game_id)).await;
        assert_eq!(resp.status_code(), StatusCode::FORBIDDEN);

        // Start it: state moves to Betting. Still in progress.
        server
            .post(&format!("/games/{}/transition", game.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await
            .assert_status_ok();
        let resp = server.get(&format!("/games/{}/replay", game.game_id)).await;
        assert_eq!(resp.status_code(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn replay_returns_transcript_for_aborted_game() {
        // A timed game with a zero-second clock fires the first-round
        // betting timeout and aborts the game; the replay then becomes
        // available because state is terminal.
        let server = test_app();
        let create = server
            .post("/games")
            .json(&serde_json::json!({
                "max_points": 100,
                "timer_config": { "initial_time_secs": 0, "increment_secs": 0 }
            }))
            .await;
        create.assert_status_ok();
        let game: CreateGameResponse = create.json();
        server
            .post(&format!("/games/{}/transition", game.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await
            .assert_status_ok();

        // Give the actor's timer task a moment to fire the abort.
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        let resp = server.get(&format!("/games/{}/replay", game.game_id)).await;
        resp.assert_status_ok();
        assert_eq!(
            resp.header("content-type").to_str().unwrap(),
            "text/plain; charset=utf-8",
        );
        let body = resp.text();
        assert!(!body.is_empty(), "transcript should be non-empty");
        // Sanity check: real transcripts include header lines.
        assert!(
            body.contains("\n"),
            "transcript looks structurally suspect: {body:?}"
        );
    }

    #[tokio::test]
    async fn test_health_returns_ok() {
        let server = test_app();
        let response = server.get("/health").await;
        response.assert_status_ok();
        response.assert_text("ok");
    }

    #[test]
    fn session_db_url_is_a_sibling_file_not_the_game_db() {
        // The session store must not open the game DB file (WAL-lock contention
        // during a deploy swap). It lives next to it as sessions.sqlite.
        assert_eq!(
            session_db_url(Some("/data/games.sqlite")),
            "sqlite:/data/sessions.sqlite?mode=rwc"
        );
        assert_eq!(
            session_db_url(Some("dev.sqlite")),
            "sqlite:sessions.sqlite?mode=rwc"
        );
        // In-memory / no-DB runs stay in memory.
        assert_eq!(session_db_url(None), "sqlite::memory:");
        assert_eq!(session_db_url(Some(":memory:")), "sqlite::memory:");
    }

    #[tokio::test]
    async fn test_cors_layer_supports_credentialed_preflight() {
        use axum::http::{Method, Request, header};
        use tower::ServiceExt;

        let cors = build_cors_layer(&["https://app.wlim.dev".to_string()]).expect("cors layer");
        let app = axum::Router::new()
            .route("/health", axum::routing::get(|| async { "ok" }))
            .layer(cors);

        // Browser preflight for a credentialed fetch (credentials: 'include').
        let preflight = Request::builder()
            .method(Method::OPTIONS)
            .uri("/health")
            .header(header::ORIGIN, "https://app.wlim.dev")
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
            .body(axum::body::Body::empty())
            .unwrap();
        let response = app.oneshot(preflight).await.unwrap();

        let headers = response.headers();
        assert_eq!(
            headers
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .map(|v| v.to_str().unwrap()),
            Some("https://app.wlim.dev"),
            "exact origin must be echoed (wildcard is invalid with credentials)"
        );
        assert_eq!(
            headers
                .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
                .map(|v| v.to_str().unwrap()),
            Some("true"),
            "credentialed session-cookie requests require allow-credentials"
        );
        assert!(
            headers.contains_key(header::ACCESS_CONTROL_ALLOW_METHODS),
            "preflight must answer with allowed methods"
        );
        assert!(
            headers.contains_key(header::ACCESS_CONTROL_ALLOW_HEADERS),
            "preflight must answer with allowed headers"
        );
    }

    // ---- WebSocket integration tests ----------------------------------
    //
    // These drive a real WS upgrade through the router, which is the only
    // way the body of `handle_game_ws` (in bin/server/ws.rs) executes —
    // REST-only test requests stop at `ws.on_upgrade(...)` and never run
    // the inner future. Without these, ws.rs reports 0% line coverage
    // despite the code being well-exercised in production.

    /// Helper: drain WS events until one whose `event` field matches `wanted`
    /// (or any value in `wanted_any`) is found. Fails the test if no match
    /// arrives within a reasonable budget. Tolerant of presence/snapshot
    /// ordering ambiguity under different transports.
    async fn ws_recv_event_of(
        ws: &mut axum_test::TestWebSocket,
        wanted: &str,
    ) -> serde_json::Value {
        for _ in 0..16 {
            let msg: serde_json::Value = ws.receive_json().await;
            if msg["event"] == wanted {
                return msg;
            }
        }
        panic!("did not receive a `{wanted}` event within budget");
    }

    #[tokio::test]
    async fn ws_player_receives_snapshot_then_state_change_on_transition() {
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        let path = format!(
            "/games/{}/ws?player_id={}",
            create.game_id, create.player_ids[0]
        );
        let mut ws = server.get_websocket(&path).await.into_websocket().await;

        // Initial snapshot: seq 0, state NotStarted.
        let snap = ws_recv_event_of(&mut ws, "state_changed").await;
        assert_eq!(snap["seq"], 0);
        assert_eq!(snap["state"], "NotStarted");

        // Trigger a transition via REST.
        server
            .post(&format!("/games/{}/transition", create.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await
            .assert_status_ok();

        // After the snapshot, the next state_changed event must reflect
        // the Start transition (state moves into Betting). The State enum
        // is externally tagged by serde, so Betting(seat_idx) serializes
        // to `{"Betting": <idx>}`.
        let event = ws_recv_event_of(&mut ws, "state_changed").await;
        assert!(event["state"]["Betting"].is_number());
    }

    #[tokio::test]
    async fn ws_with_since_param_replays_buffered_events_instead_of_snapshot() {
        // Drive the WS catch-up branch (handler's `Some(events)` arm) by
        // pushing a transition into the broadcast then connecting with
        // `?since=0`. The actor's ring buffer still holds seq 0, so the
        // server should replay it as a state_changed event with seq 0
        // (the Start transition) instead of sending a fresh snapshot.
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        // Drive seq forward: Start moves the actor's next_seq from 0 → 1.
        server
            .post(&format!("/games/{}/transition", create.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await
            .assert_status_ok();

        // Subscribe with since=0 — actor builds a catch_up containing the
        // single buffered StateChanged at seq 0.
        let path = format!(
            "/games/{}/ws?player_id={}&since=0",
            create.game_id, create.player_ids[0],
        );
        let mut ws = server.get_websocket(&path).await.into_websocket().await;

        let event = ws_recv_event_of(&mut ws, "state_changed").await;
        assert_eq!(event["seq"], 0);
        // Catch-up event carries the buffered state at that seq, which is
        // post-Start (Betting), not the pre-Start NotStarted that a fresh
        // snapshot would deliver.
        assert!(event["state"]["Betting"].is_number());
    }

    #[tokio::test]
    async fn ws_with_future_since_falls_back_to_fresh_snapshot() {
        // since=999 is beyond the actor's current_seq → catch_up is None
        // and the handler sends a snapshot instead. Verifies the fallback
        // arm of build_subscription.
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        let path = format!(
            "/games/{}/ws?player_id={}&since=999",
            create.game_id, create.player_ids[0],
        );
        let mut ws = server.get_websocket(&path).await.into_websocket().await;
        let snap = ws_recv_event_of(&mut ws, "state_changed").await;
        // Fresh snapshot delivers the current state, which is still NotStarted.
        assert_eq!(snap["state"], "NotStarted");
    }

    #[tokio::test]
    async fn ws_without_player_id_is_rejected_unauthorized() {
        // Game streams are private to seated players: a connection that
        // doesn't claim a seat is refused before the upgrade happens.
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        let response = server
            .get_websocket(&format!("/games/{}/ws", create.game_id))
            .await;
        response.assert_status_unauthorized();
    }

    #[tokio::test]
    async fn ws_with_unowned_player_id_is_rejected_forbidden() {
        let mut server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        // Drop the creator's anon-session cookie: the connect below runs as
        // a fresh identity that owns no seat in this game, even though it
        // presents a valid player_id (which is visible in game state).
        server.clear_cookies();
        let response = server
            .get_websocket(&format!(
                "/games/{}/ws?player_id={}",
                create.game_id, create.player_ids[0]
            ))
            .await;
        response.assert_status_forbidden();
    }

    #[tokio::test]
    async fn transition_by_non_seated_caller_is_forbidden() {
        // Transitions apply to whichever player the engine says is current,
        // so without this gate anyone who learns a game_id could play other
        // people's turns (including rated games).
        let mut server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        // Fresh identity that owns no seat in the game.
        server.clear_cookies();
        let response = server
            .post(&format!("/games/{}/transition", create.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await;
        response.assert_status_forbidden();
    }

    #[tokio::test]
    async fn transition_on_another_players_turn_is_forbidden() {
        let (server, store) = test_app_with_store();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        server
            .post(&format!("/games/{}/transition", create.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await
            .assert_status_ok();

        // Find whose turn it is and hand that seat to a different identity,
        // simulating a multi-human game (challenge/matchmaking bind each
        // seat to its own player's identity at join time).
        let game: serde_json::Value = server
            .get(&format!("/games/{}", create.game_id))
            .await
            .json();
        let current_id =
            uuid::Uuid::parse_str(game["current_player_id"].as_str().unwrap()).unwrap();
        let seat_index = create
            .player_ids
            .iter()
            .position(|p| *p == current_id)
            .unwrap();
        store
            .insert_game_seat(
                create.game_id,
                seat_index as i32,
                current_id,
                spades_server::auth::game_seats::SeatOwner {
                    user_id: None,
                    anon_user_id: Some(uuid::Uuid::new_v4()),
                    is_bot: false,
                },
            )
            .unwrap();

        // The caller still owns the other three seats, but it is not their
        // turn — betting for the current player must be refused.
        let response = server
            .post(&format!("/games/{}/transition", create.game_id))
            .json(&serde_json::json!({"type": "bet", "amount": 3}))
            .await;
        response.assert_status_forbidden();
    }

    #[tokio::test]
    async fn ws_subscriber_receives_chat_messages() {
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        let path = format!(
            "/games/{}/ws?player_id={}",
            create.game_id, create.player_ids[0]
        );
        let mut ws = server.get_websocket(&path).await.into_websocket().await;

        // Wait for the initial snapshot before sending chat so the chat
        // arrives during the WS streaming loop.
        let _snap = ws_recv_event_of(&mut ws, "state_changed").await;

        // Owner posts a chat message.
        server
            .post(&format!("/games/{}/chat", create.game_id))
            .json(&serde_json::json!({
                "player_id": create.player_ids[0],
                "content": "ready",
            }))
            .await
            .assert_status(StatusCode::ACCEPTED);

        let chat = ws_recv_event_of(&mut ws, "chat_message").await;
        assert_eq!(chat["content"], "ready");
        assert_eq!(
            chat["player_id"].as_str().unwrap(),
            create.player_ids[0].to_string(),
        );
    }

    // ---- WebSocket resilience: disconnect / reconnect / timeout --------
    //
    // These exercise the lifecycle tail of `handle_game_ws` that the
    // happy-path tests above never reach: presence cleanup on disconnect,
    // catch-up after a real reconnect, timeout aborts arriving on a live
    // socket, and the client-driven keepalive path.

    #[tokio::test]
    async fn ws_disconnect_broadcasts_updated_presence_to_remaining_subscriber() {
        // Two players watch the same game. When A's socket drops, the
        // disconnect tail (`ws.rs` ~225) must decrement A's connection count
        // and broadcast a fresh presence snapshot — which B, still connected,
        // observes as A `connected: false`.
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        let path_a = format!(
            "/games/{}/ws?player_id={}",
            create.game_id, create.player_ids[0]
        );
        let path_b = format!(
            "/games/{}/ws?player_id={}",
            create.game_id, create.player_ids[1]
        );
        let ws_a = server.get_websocket(&path_a).await.into_websocket().await;
        let mut ws_b = server.get_websocket(&path_b).await.into_websocket().await;

        // Drop A. A graceful close drives the server's `Message::Close` arm.
        ws_a.close().await;

        // B drains until it sees A reported as disconnected.
        let a_id = create.player_ids[0].to_string();
        for _ in 0..32 {
            let msg: serde_json::Value = ws_b.receive_json().await;
            if msg["event"] != "presence_changed" {
                continue;
            }
            let a_disconnected = msg["players"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|p| p["player_id"] == a_id && p["connected"] == false);
            if a_disconnected {
                return;
            }
        }
        panic!("B never observed A's disconnect in a presence snapshot");
    }

    #[tokio::test]
    async fn ws_reconnect_with_since_replays_missed_events_then_resumes_live() {
        // Full reconnect cycle: connect, drop, miss an event, reconnect with
        // `?since=` to replay it, then confirm the new socket is live by
        // receiving a freshly-broadcast event.
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        let path = format!(
            "/games/{}/ws?player_id={}",
            create.game_id, create.player_ids[0]
        );
        let mut ws = server.get_websocket(&path).await.into_websocket().await;
        let snap = ws_recv_event_of(&mut ws, "state_changed").await;
        assert_eq!(snap["seq"], 0);
        assert_eq!(snap["state"], "NotStarted");
        ws.close().await;

        // Missed while disconnected: the Start transition is broadcast at seq 0.
        server
            .post(&format!("/games/{}/transition", create.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await
            .assert_status_ok();

        // Reconnect asking for everything from seq 0: the ring buffer still
        // holds it, so it replays as catch-up (Betting), not a fresh snapshot.
        let reconnect = format!(
            "/games/{}/ws?player_id={}&since=0",
            create.game_id, create.player_ids[0]
        );
        let mut ws = server
            .get_websocket(&reconnect)
            .await
            .into_websocket()
            .await;
        let replayed = ws_recv_event_of(&mut ws, "state_changed").await;
        assert_eq!(replayed["seq"], 0);
        assert!(replayed["state"]["Betting"].is_number());

        // Prove the reconnected socket is fully live: a new chat fans out to it.
        server
            .post(&format!("/games/{}/chat", create.game_id))
            .json(&serde_json::json!({
                "player_id": create.player_ids[0],
                "content": "back online",
            }))
            .await
            .assert_status(StatusCode::ACCEPTED);
        let chat = ws_recv_event_of(&mut ws, "chat_message").await;
        assert_eq!(chat["content"], "back online");
    }

    #[tokio::test]
    async fn ws_first_round_timeout_delivers_game_aborted_to_live_subscriber() {
        // A zero-second clock fires the first-round betting timeout the moment
        // the game starts; a connected subscriber must receive `game_aborted`
        // over the wire (not just see the transcript become available).
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({
                "max_points": 100,
                "timer_config": { "initial_time_secs": 0, "increment_secs": 0 }
            }))
            .await
            .json();

        let path = format!(
            "/games/{}/ws?player_id={}",
            create.game_id, create.player_ids[0]
        );
        let mut ws = server.get_websocket(&path).await.into_websocket().await;
        let _snap = ws_recv_event_of(&mut ws, "state_changed").await;

        server
            .post(&format!("/games/{}/transition", create.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await
            .assert_status_ok();

        let aborted = ws_recv_event_of(&mut ws, "game_aborted").await;
        assert_eq!(aborted["game_id"], create.game_id.to_string());
        assert!(
            aborted["reason"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .contains("first round"),
            "unexpected abort reason: {:?}",
            aborted["reason"]
        );
    }

    #[tokio::test]
    async fn ws_responds_to_client_ping_with_pong() {
        // Client-driven keepalive: a Ping must come back as a Pong (ws.rs ~213),
        // keeping an otherwise-idle connection alive. Game-state/presence Text
        // frames may interleave, so drain until the Pong arrives.
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        let path = format!(
            "/games/{}/ws?player_id={}",
            create.game_id, create.player_ids[0]
        );
        let mut ws = server.get_websocket(&path).await.into_websocket().await;

        ws.send_message(axum_test::WsMessage::Ping(vec![7u8, 8, 9].into()))
            .await;

        for _ in 0..16 {
            if let axum_test::WsMessage::Pong(payload) = ws.receive_message().await {
                assert_eq!(payload.as_ref(), &[7u8, 8, 9]);
                return;
            }
        }
        panic!("never received a Pong in response to the client Ping");
    }

    #[tokio::test]
    async fn ws_closes_when_game_is_deleted() {
        // Deleting a game drops its actor handle; the actor exits, its
        // broadcast sender drops, and the WS handler's `rx.recv()` returns
        // `Closed` and tears the socket down (ws.rs ~187). The client must
        // observe the stream end rather than hang forever.
        //
        // (Regression guard: the actor used to retain a strong self-reference
        // that kept it — and this socket — alive after delete. See the weak
        // `self_tx` in game_actor.rs.)
        let server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        let path = format!(
            "/games/{}/ws?player_id={}",
            create.game_id, create.player_ids[0]
        );
        let mut ws = server.get_websocket(&path).await.into_websocket().await;
        let _snap = ws_recv_event_of(&mut ws, "state_changed").await;

        // The creating session owns a seat, so it is authorized to delete.
        server
            .delete(&format!("/games/{}", create.game_id))
            .await
            .assert_status(StatusCode::NO_CONTENT);

        // The socket must close. `receive_message` panics if the stream yields
        // an error/None, so bound the wait in a timeout and treat a clean Close
        // frame as success; drain any in-flight Text frames first.
        let closed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                match ws.receive_message().await {
                    axum_test::WsMessage::Close(_) => break true,
                    _ => continue,
                }
            }
        })
        .await;
        assert_eq!(closed, Ok(true), "socket did not close after game deletion");
    }

    // ---- matchmaking SSE handler: input validation ---------------------
    //
    // The seek SSE stream itself is hard to drive from axum-test (it never
    // terminates on its own), but the validation branches before the
    // stream yields a 4xx synchronously. Exercising them locks in the
    // contract for the front-end and bumps coverage of handlers/matchmaking.rs.

    #[tokio::test]
    async fn seek_rejects_non_positive_max_points() {
        let server = test_app();
        let response = server
            .post("/matchmaking/seek")
            .json(&serde_json::json!({
                "max_points": 0,
                "timer_config": {"initial_time_secs": 60, "increment_secs": 5},
            }))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn seek_rejects_invalid_name() {
        let server = test_app();
        let response = server
            .post("/matchmaking/seek")
            .json(&serde_json::json!({
                "max_points": 500,
                "timer_config": {"initial_time_secs": 60, "increment_secs": 5},
                "name": "",
            }))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }

    // ---- challenges handler: validation + lifecycle --------------------

    #[tokio::test]
    async fn create_challenge_rejects_non_positive_max_points() {
        let server = test_app();
        let response = server
            .post("/challenges")
            .json(&serde_json::json!({
                "max_points": -1,
            }))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn create_challenge_rejects_invalid_creator_name() {
        let server = test_app();
        let response = server
            .post("/challenges")
            .json(&serde_json::json!({
                "max_points": 500,
                "creator_name": "",
            }))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn join_challenge_rejects_invalid_seat() {
        let server = test_app();
        let response = server
            .post(&format!("/challenges/{}/join/Z", Uuid::new_v4()))
            .json(&serde_json::json!({}))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn join_challenge_rejects_invalid_name() {
        let server = test_app();
        let response = server
            .post(&format!("/challenges/{}/join/A", Uuid::new_v4()))
            .json(&serde_json::json!({"name": ""}))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn join_challenge_returns_404_when_challenge_missing() {
        let server = test_app();
        let response = server
            .post(&format!("/challenges/{}/join/A", Uuid::new_v4()))
            .json(&serde_json::json!({}))
            .await;
        response.assert_status(StatusCode::NOT_FOUND);
    }

    /// Helper: open an SSE POST, parse the first `event: <name>\ndata: <json>`
    /// frame, then drop the connection. The reqwest Response's body stream is
    /// dropped on return, which the server treats as a closed connection —
    /// any Drop-based cleanup (e.g. SeekGuard / ChallengeGuard) runs.
    async fn sse_first_event(
        server: &TestServer,
        path: &str,
        body: serde_json::Value,
    ) -> (String, serde_json::Value) {
        let mut resp = server
            .reqwest_post(path)
            .json(&body)
            .send()
            .await
            .expect("post failed");
        assert!(
            resp.status().is_success(),
            "SSE POST not 2xx: {}",
            resp.status()
        );
        let mut buf = String::new();
        while let Some(chunk) = resp.chunk().await.expect("chunk read failed") {
            buf.push_str(&String::from_utf8_lossy(&chunk));
            if buf.contains("\n\n") {
                break;
            }
        }
        // Parse the first SSE frame.
        let frame = buf.split("\n\n").next().unwrap();
        let mut event_name = String::new();
        let mut data = String::new();
        for line in frame.lines() {
            if let Some(rest) = line.strip_prefix("event:") {
                event_name = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("data:") {
                data.push_str(rest.trim());
            }
        }
        let value: serde_json::Value = serde_json::from_str(&data).expect("invalid SSE data JSON");
        (event_name, value)
    }

    #[tokio::test]
    async fn create_challenge_emits_initial_event_and_cancel_403_for_stranger() {
        // Drive the create-challenge SSE stream just far enough to extract
        // the new `challenge_id`, then drop the stream. Clear cookies to
        // simulate a different session and assert the handler maps
        // `NotCreator` → 403. The positive cancel path is covered by
        // `create_challenge_creator_can_cancel`.
        let mut server = test_app();
        let (event_name, value) = sse_first_event(
            &server,
            "/challenges",
            serde_json::json!({
                "max_points": 500,
                "creator_seat": "A",
            }),
        )
        .await;
        assert_eq!(event_name, "challenge_created");
        let challenge_id = value["challenge_id"].as_str().expect("challenge_id");

        server.clear_cookies();
        server
            .delete(&format!("/challenges/{}", challenge_id))
            .await
            .assert_status(StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_challenge_by_short_id_returns_full_status_on_match() {
        let server = test_app();
        let (_, value) = sse_first_event(
            &server,
            "/challenges",
            serde_json::json!({
                "max_points": 500,
                "creator_seat": "A",
            }),
        )
        .await;
        let short_id = value["short_id"].as_str().expect("short_id");

        let response = server
            .get(&format!("/challenges/by-short-id/{}", short_id))
            .await;
        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["short_id"], short_id);
    }

    #[tokio::test]
    async fn test_readyz_returns_ok() {
        let server = test_app();
        let response = server.get("/readyz").await;
        response.assert_status_ok();
        response.assert_text("ok");
    }

    #[tokio::test]
    async fn test_catch_panic_layer_isolates_failures() {
        use axum::Router;
        use axum::routing::get;
        use tower_http::catch_panic::CatchPanicLayer;

        async fn healthy() -> &'static str {
            "ok"
        }
        async fn boom() -> &'static str {
            panic!("intentional test panic")
        }

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

        async fn echo(_body: axum::body::Bytes) -> &'static str {
            "ok"
        }

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

        let app: Router =
            Router::new()
                .route("/slow", get(slow))
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    std::time::Duration::from_millis(50),
                ));
        let server = TestServer::new(app).unwrap();

        let resp = server.get("/slow").await;
        assert_eq!(resp.status_code(), StatusCode::REQUEST_TIMEOUT);
    }

    #[test]
    fn server_event_resync_serializes_with_tag() {
        use crate::dto::ServerEvent;
        let event = ServerEvent::Resync {
            reason: "lagged 5".to_string(),
        };
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
                spades_server::game_manager::PlayerNameEntry {
                    player_id: Uuid::nil(),
                    name: None,
                },
                spades_server::game_manager::PlayerNameEntry {
                    player_id: Uuid::nil(),
                    name: None,
                },
                spades_server::game_manager::PlayerNameEntry {
                    player_id: Uuid::nil(),
                    name: None,
                },
                spades_server::game_manager::PlayerNameEntry {
                    player_id: Uuid::nil(),
                    name: None,
                },
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
        let parsed: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
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
        use spades::GameTransition;
        use spades_server::game_manager::GameManager;
        use tokio::sync::broadcast;

        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        let mut sub = manager.subscribe(response.game_id, None).await.unwrap();

        manager
            .make_transition(response.game_id, GameTransition::Start)
            .await
            .unwrap();
        for i in 0..80 {
            manager
                .set_player_name(
                    response.game_id,
                    response.player_ids[0],
                    Some(format!("p{i}")),
                )
                .await
                .unwrap();
        }

        let mut saw_lagged = false;
        for _ in 0..200 {
            match sub.rx.try_recv() {
                // Events are now delivered as pre-serialized `Arc<CachedEvent>`;
                // the variant no longer matters here, only that the buffer
                // overflows into Lagged.
                Ok(_) => continue,
                Err(broadcast::error::TryRecvError::Lagged(_)) => {
                    saw_lagged = true;
                    break;
                }
                Err(_) => break,
            }
        }
        assert!(
            saw_lagged,
            "expected Lagged after overflowing the broadcast buffer"
        );
    }

    fn env_map<const N: usize>(pairs: [(&str, &str); N]) -> impl Fn(&str) -> Option<String> {
        let m: std::collections::HashMap<String, String> = pairs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name: &str| m.get(name).cloned()
    }

    #[test]
    fn config_warnings_empty_when_nothing_set() {
        let warnings = super::collect_config_warnings(env_map([]));
        assert!(warnings.is_empty());
    }

    #[test]
    fn config_warnings_smtp_partial_flags_each_missing_var() {
        let warnings = super::collect_config_warnings(env_map([("SMTP_HOST", "mail.example.com")]));
        // All three of SMTP_USER / SMTP_PASS / SMTP_FROM are missing.
        assert_eq!(warnings.len(), 3);
        for v in ["SMTP_USER", "SMTP_PASS", "SMTP_FROM"] {
            assert!(
                warnings.iter().any(|w| w.contains(v)),
                "expected a warning mentioning {v}; got {warnings:?}",
            );
        }
    }

    #[test]
    fn config_warnings_smtp_complete_is_quiet() {
        let warnings = super::collect_config_warnings(env_map([
            ("SMTP_HOST", "mail.example.com"),
            ("SMTP_USER", "u"),
            ("SMTP_PASS", "p"),
            ("SMTP_FROM", "noreply@example.com"),
        ]));
        assert!(
            warnings.is_empty(),
            "no warnings when all SMTP vars set; got {warnings:?}"
        );
    }

    #[test]
    fn config_warnings_partial_oauth_flags_missing_side() {
        let warnings = super::collect_config_warnings(env_map([("GOOGLE_OAUTH_CLIENT_ID", "abc")]));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("GOOGLE_OAUTH_CLIENT_SECRET"));

        let warnings =
            super::collect_config_warnings(env_map([("GITHUB_OAUTH_CLIENT_SECRET", "xyz")]));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("GITHUB_OAUTH_CLIENT_ID"));
    }

    #[test]
    fn config_warnings_complete_oauth_is_quiet() {
        let warnings = super::collect_config_warnings(env_map([
            ("GOOGLE_OAUTH_CLIENT_ID", "g_id"),
            ("GOOGLE_OAUTH_CLIENT_SECRET", "g_sec"),
        ]));
        assert!(
            warnings.is_empty(),
            "no warnings when both sides set; got {warnings:?}"
        );
    }

    #[test]
    fn config_warnings_redirect_url_must_have_http_scheme() {
        let warnings = super::collect_config_warnings(env_map([(
            "OAUTH_REDIRECT_BASE_URL",
            "example.com/cb",
        )]));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("OAUTH_REDIRECT_BASE_URL"));

        let warnings = super::collect_config_warnings(env_map([(
            "OAUTH_REDIRECT_BASE_URL",
            "http://example.com",
        )]));
        assert!(warnings.is_empty());

        let warnings = super::collect_config_warnings(env_map([(
            "OAUTH_REDIRECT_BASE_URL",
            "https://example.com",
        )]));
        assert!(warnings.is_empty());
    }

    #[tokio::test]
    async fn idempotency_key_replays_same_transition_outcome() {
        // First POST starts the game successfully. A second POST with the
        // same Idempotency-Key must replay that 200 rather than re-running
        // the transition (which would fail with "AlreadyStarted"). A third
        // POST without the key proves the second call's success came from
        // the cache, not from lenient game state.
        let server = test_app();
        let create = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await;
        create.assert_status_ok();
        let game: CreateGameResponse = create.json();

        let first = server
            .post(&format!("/games/{}/transition", game.game_id))
            .add_header("idempotency-key", "retry-1")
            .json(&serde_json::json!({"type": "start"}))
            .await;
        first.assert_status_ok();
        let first_body: serde_json::Value = first.json();
        assert_eq!(first_body["success"], true);
        let first_result = first_body["result"].as_str().unwrap().to_string();

        let second = server
            .post(&format!("/games/{}/transition", game.game_id))
            .add_header("idempotency-key", "retry-1")
            .json(&serde_json::json!({"type": "start"}))
            .await;
        second.assert_status_ok();
        let second_body: serde_json::Value = second.json();
        assert_eq!(second_body["success"], true);
        assert_eq!(
            second_body["result"].as_str().unwrap(),
            first_result,
            "replay returns the identical cached response",
        );

        let fresh = server
            .post(&format!("/games/{}/transition", game.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await;
        assert!(
            !fresh.status_code().is_success(),
            "without the idempotency key, the retry should reach the engine \
             and fail with 'AlreadyStarted' — got {} which would mean the \
             game allows double-start",
            fresh.status_code(),
        );
    }

    #[tokio::test]
    async fn create_game_returns_429_after_per_user_burst() {
        // create_game burst is 10, so the 11th rapid POST from the same
        // session should be rejected with 429 — same anon_id across requests
        // because axum-test preserves session cookies on the TestServer.
        let server = test_app();
        let mut statuses = Vec::new();
        for _ in 0..15 {
            let resp = server
                .post("/games")
                .json(&serde_json::json!({"max_points": 500}))
                .await;
            statuses.push(resp.status_code());
        }
        assert!(
            statuses.contains(&StatusCode::TOO_MANY_REQUESTS),
            "expected at least one 429 within 15 rapid POSTs; got {statuses:?}",
        );
        // The first 10 should still have succeeded (burst capacity).
        assert!(
            statuses.iter().take(10).all(|s| *s == StatusCode::OK),
            "first 10 should be within burst; got {:?}",
            &statuses[..10],
        );
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
    async fn test_list_games_route_removed() {
        // GET /games used to enumerate every game's UUID — removed because
        // the frontend never called it and it leaked an enumeration surface
        // to unauthenticated callers.
        let server = test_app();
        let response = server.get("/games").await;
        response.assert_status(StatusCode::METHOD_NOT_ALLOWED);
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

        let response = server
            .delete(&format!("/games/{}", create_resp.game_id))
            .await;
        response.assert_status(StatusCode::NO_CONTENT);

        // Verify it's gone — subsequent GET returns 404.
        let after = server.get(&format!("/games/{}", create_resp.game_id)).await;
        after.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_game_not_found() {
        let server = test_app();
        let response = server.delete(&format!("/games/{}", Uuid::new_v4())).await;
        response.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_game_requires_seat_ownership() {
        // A different anon session on the same server must not be able to
        // delete a game it doesn't have a seat in. The positive path is
        // covered by `test_delete_game`.
        let mut server = test_app();
        let create_resp: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();

        server.clear_cookies();
        server
            .delete(&format!("/games/{}", create_resp.game_id))
            .await
            .assert_status(StatusCode::FORBIDDEN);
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
    async fn get_hand_returns_403_for_cross_session_request() {
        // Single TestServer (one AppState — one GameManager, one
        // auth_store), two logical sessions: session A creates the game
        // and owns all four seats, then `clear_cookies` simulates session
        // B requesting the same hand. Without the seat-owner gate this
        // used to leak every player's hand to anyone with the URL.
        let mut server = test_app();
        let create: CreateGameResponse = server
            .post("/games")
            .json(&serde_json::json!({"max_points": 500}))
            .await
            .json();
        server
            .post(&format!("/games/{}/transition", create.game_id))
            .json(&serde_json::json!({"type": "start"}))
            .await
            .assert_status_ok();

        // Session A (the owner) succeeds.
        server
            .get(&format!(
                "/games/{}/players/{}/hand",
                create.game_id, create.player_ids[0]
            ))
            .await
            .assert_status_ok();

        // Wipe the session cookie. The next request mints a fresh anon
        // session whose anon_id won't match the seat's recorded owner.
        server.clear_cookies();
        let cross = server
            .get(&format!(
                "/games/{}/players/{}/hand",
                create.game_id, create.player_ids[0]
            ))
            .await;
        assert_eq!(cross.status_code(), StatusCode::FORBIDDEN);
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
        let response = server.get(&format!("/challenges/{}", Uuid::new_v4())).await;
        response.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_cancel_challenge_not_found() {
        let server = test_app();
        let response = server
            .delete(&format!("/challenges/{}", Uuid::new_v4()))
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
    async fn create_game_with_two_humans_seats_bots_in_opponent_team() {
        // num_humans=2 puts the two humans on seats 0 and 2 (team A); seats
        // 1 and 3 are bots on team B. The game must auto-start so the
        // bots play out their turns rather than blocking on a human.
        let app = test_app();
        let response = app
            .post("/games")
            .json(&serde_json::json!({ "max_points": 500, "num_humans": 2 }))
            .await;
        response.assert_status_ok();
        let body: CreateGameResponse = response.json();
        let state: GameStateResponse = app.get(&format!("/games/{}", body.game_id)).await.json();
        // After auto-start, the game progressed past NotStarted.
        assert_ne!(state.state, spades::State::NotStarted);
    }

    #[tokio::test]
    async fn create_game_rejects_invalid_num_humans() {
        let app = test_app();
        let response = app
            .post("/games")
            .json(&serde_json::json!({ "max_points": 500, "num_humans": 3 }))
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_game_by_short_id_returns_game_state_when_found() {
        // create_game returns the game_id; GameStateResponse carries the
        // short_id. The by-short-id lookup must round-trip back to the
        // same game.
        let app = test_app();
        let body: CreateGameResponse = app
            .post("/games")
            .json(&serde_json::json!({ "max_points": 500 }))
            .await
            .json();
        let state: GameStateResponse = app.get(&format!("/games/{}", body.game_id)).await.json();
        let resp = app
            .get(&format!("/games/by-short-id/{}", state.short_id))
            .await;
        resp.assert_status_ok();
        let again: GameStateResponse = resp.json();
        assert_eq!(again.game_id, body.game_id);
    }

    #[tokio::test]
    async fn get_game_by_short_id_returns_404_for_unknown() {
        let app = test_app();
        let resp = app.get("/games/by-short-id/zzz-not-a-real-short-id").await;
        resp.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_game_by_player_url_returns_404_for_garbage_url() {
        let app = test_app();
        let resp = app.get("/games/by-player-url/not-base64-decodable").await;
        resp.assert_status(StatusCode::NOT_FOUND);
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

        let state_response = app.get(&format!("/games/{}", body.game_id)).await;
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
        let human_entry = snap
            .players
            .iter()
            .find(|p| p.player_id == human_pid)
            .unwrap();
        assert!(!human_entry.connected);
        // All 3 AI players should be connected
        let ai_connected: Vec<_> = snap
            .players
            .iter()
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
