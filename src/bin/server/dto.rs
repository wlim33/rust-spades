use oasgen::OaSchema;
use serde::{Deserialize, Serialize};
use spades::game_manager::{GameStateResponse, HandResponse};
use spades::{Card, TimerConfig};
use uuid::Uuid;

pub fn default_max_points() -> i32 {
    500
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserSession {
    pub user_id: Uuid,
    pub display_name: Option<String>,
}

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
    StateChanged(GameStateResponse),
    GameAborted { game_id: Uuid, reason: String },
    PresenceChanged(PresenceSnapshot),
}
