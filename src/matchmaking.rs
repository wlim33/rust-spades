use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use tokio::sync::{oneshot, broadcast};
use crate::game_manager::GameManager;
use crate::GameTransition;

/// Result sent to matched players
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    pub game_id: Uuid,
    pub player_id: Uuid,
    pub player_ids: [Uuid; 4],
}

/// SSE event sent to lobby members
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LobbyEvent {
    LobbyUpdate { lobby_id: Uuid, players: usize },
    GameStart(MatchResult),
}

/// Summary of seeks waiting for a given max_points
#[derive(Debug, Serialize, Deserialize)]
pub struct SeekSummary {
    pub max_points: i32,
    pub waiting: usize,
}

/// Summary of an open lobby
#[derive(Debug, Serialize, Deserialize)]
pub struct LobbySummary {
    pub lobby_id: Uuid,
    pub max_points: i32,
    pub players: usize,
}

/// Errors from matchmaking operations
#[derive(Debug, Serialize, Deserialize)]
pub enum MatchmakingError {
    LobbyNotFound,
    LobbyFull,
    LockError,
    GameCreationFailed(String),
}

struct PendingSeek {
    player_id: Uuid,
    max_points: i32,
    sender: oneshot::Sender<MatchResult>,
}

struct LobbyPlayer {
    player_id: Uuid,
}

struct Lobby {
    lobby_id: Uuid,
    creator_id: Uuid,
    max_points: i32,
    players: Vec<LobbyPlayer>,
    broadcast_tx: broadcast::Sender<LobbyEvent>,
}

/// Manages matchmaking: seek queue and lobbies.
#[derive(Clone)]
pub struct Matchmaker {
    game_manager: GameManager,
    seek_queue: Arc<Mutex<Vec<PendingSeek>>>,
    lobbies: Arc<RwLock<HashMap<Uuid, Lobby>>>,
}

impl Matchmaker {
    pub fn new(game_manager: GameManager) -> Self {
        Matchmaker {
            game_manager,
            seek_queue: Arc::new(Mutex::new(Vec::new())),
            lobbies: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}
