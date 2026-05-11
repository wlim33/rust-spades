use oasgen::OaSchema;
use serde::{Deserialize, Serialize};
use spades_server::game_manager::{GameStateResponse, HandResponse};
use spades::{Card, TimerConfig};
use uuid::Uuid;

pub fn default_max_points() -> i32 {
    500
}

// Re-export the canonical session payload from the library so this binary's
// handlers can continue to import `crate::dto::UserSession` unchanged.
pub use spades_server::auth::session_ext::UserSession;

#[derive(Debug, Serialize, OaSchema)]
pub struct SessionPlayerResponse {
    pub user_id: Uuid,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize, OaSchema)]
pub struct SetDisplayNameRequest {
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, OaSchema)]
pub struct CreateGameRequest {
    #[serde(default = "default_max_points")]
    pub max_points: i32,
    pub timer_config: Option<TimerConfig>,
    pub num_humans: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize, OaSchema)]
pub struct TransitionRequest {
    #[serde(flatten)]
    pub transition: TransitionType,
}

#[derive(Debug, Serialize, Deserialize, OaSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransitionType {
    Start,
    Bet { amount: i32 },
    Card { card: Card },
}

#[derive(Debug, Serialize, OaSchema)]
pub struct TransitionResponse {
    pub success: bool,
    pub result: String,
}

#[derive(Debug, Serialize, Deserialize, OaSchema)]
pub struct ErrorResponse {
    pub error: String,
}

/// Convert an `AuthError` produced by `check_user` (or any rate-limit check)
/// into the tuple shape that game/match/challenge handlers return on error.
/// Loses the Retry-After header `AuthError::into_response` would set —
/// acceptable until handler return types are unified on `axum::response::Response`.
pub fn auth_err_response(
    e: spades_server::auth::AuthError,
) -> (axum::http::StatusCode, axum::response::Json<ErrorResponse>) {
    (
        e.status(),
        axum::response::Json(ErrorResponse {
            error: format!("{}", e),
        }),
    )
}

#[derive(Debug, Serialize, OaSchema)]
pub struct PlayerUrlResponse {
    pub game_id: Uuid,
    pub player_id: Uuid,
    pub player_short_id: String,
    pub game: GameStateResponse,
    pub hand: HandResponse,
}

#[derive(Debug, Serialize, Deserialize, OaSchema)]
pub struct SeekRequest {
    #[serde(default = "default_max_points")]
    pub max_points: i32,
    pub timer_config: TimerConfig,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, OaSchema)]
pub struct SetNameRequest {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, OaSchema)]
pub struct JoinChallengeRequest {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, OaSchema)]
pub struct CancelChallengeRequest {
    pub creator_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub player_id: Option<String>,
    /// Reconnection cursor: the next seq the client wants. If the server
    /// still holds events from this seq in its ring buffer, it replays them
    /// instead of sending a fresh snapshot.
    pub since: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, OaSchema)]
pub struct PlayerPresenceEntry {
    pub player_id: Uuid,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, OaSchema)]
pub struct PresenceSnapshot {
    pub game_id: Uuid,
    pub players: Vec<PlayerPresenceEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ServerEvent {
    /// Game state changed. `seq` is the per-game monotonic cursor; the
    /// initial snapshot sent on connect carries the cursor value clients
    /// should expect the next streamed event to match.
    StateChanged {
        seq: u64,
        #[serde(flatten)]
        state: GameStateResponse,
    },
    GameAborted {
        seq: u64,
        game_id: Uuid,
        reason: String,
    },
    PresenceChanged(PresenceSnapshot),
    /// Server detected that this subscription lagged past the broadcast
    /// buffer and cannot continue cleanly. Client should reconnect to
    /// receive a fresh snapshot.
    Resync { reason: String },
}
