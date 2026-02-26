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

    /// Add a seek to the queue. Returns a oneshot receiver that will receive
    /// the MatchResult when 4 players with the same max_points are matched.
    pub fn add_seek(&self, max_points: i32) -> oneshot::Receiver<MatchResult> {
        let player_id = Uuid::new_v4();
        let (tx, rx) = oneshot::channel();

        {
            let mut queue = self.seek_queue.lock().unwrap();
            queue.push(PendingSeek {
                player_id,
                max_points,
                sender: tx,
            });
        }

        self.try_match(max_points);
        rx
    }

    /// Remove a seek from the queue by player_id.
    pub fn cancel_seek(&self, player_id: Uuid) {
        let mut queue = self.seek_queue.lock().unwrap();
        queue.retain(|s| s.player_id != player_id);
    }

    /// List a summary of active seeks grouped by max_points.
    pub fn list_seeks(&self) -> Vec<SeekSummary> {
        let queue = self.seek_queue.lock().unwrap();
        let mut counts: HashMap<i32, usize> = HashMap::new();
        for seek in queue.iter() {
            *counts.entry(seek.max_points).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .map(|(max_points, waiting)| SeekSummary { max_points, waiting })
            .collect()
    }

    /// Check if there are 4 seeks with the same max_points and create a game.
    fn try_match(&self, max_points: i32) {
        let mut queue = self.seek_queue.lock().unwrap();

        let matching: Vec<usize> = queue
            .iter()
            .enumerate()
            .filter(|(_, s)| s.max_points == max_points)
            .map(|(i, _)| i)
            .collect();

        if matching.len() < 4 {
            return;
        }

        // Take the first 4 matching seeks (remove from back to preserve indices)
        let indices: Vec<usize> = matching.into_iter().take(4).collect();
        let mut seeks: Vec<PendingSeek> = Vec::with_capacity(4);
        for &i in indices.iter().rev() {
            seeks.push(queue.remove(i));
        }
        seeks.reverse();

        // Drop the lock before calling game_manager
        drop(queue);

        let player_ids: [Uuid; 4] = [
            seeks[0].player_id,
            seeks[1].player_id,
            seeks[2].player_id,
            seeks[3].player_id,
        ];

        // Create game with pre-assigned player IDs and auto-start
        let response = match self.game_manager.create_game_with_players(player_ids, max_points) {
            Ok(r) => r,
            Err(_) => return,
        };
        let _ = self.game_manager.make_transition(response.game_id, GameTransition::Start);

        // Notify all 4 players
        for seek in seeks {
            let result = MatchResult {
                game_id: response.game_id,
                player_id: seek.player_id,
                player_ids,
            };
            let _ = seek.sender.send(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_matchmaker() -> Matchmaker {
        Matchmaker::new(GameManager::new())
    }

    #[tokio::test]
    async fn test_seek_match_4_players() {
        let mm = make_matchmaker();
        let mut receivers = Vec::new();

        for _ in 0..4 {
            let rx = mm.add_seek(500);
            receivers.push(rx);
        }

        let mut game_id = None;
        for rx in receivers {
            let result = rx.await.expect("channel should not be dropped");
            if let Some(gid) = game_id {
                assert_eq!(result.game_id, gid, "all players should be in same game");
            } else {
                game_id = Some(result.game_id);
            }
            assert_eq!(result.player_ids.len(), 4);
        }
    }

    #[tokio::test]
    async fn test_seek_no_match_with_3_players() {
        let mm = make_matchmaker();

        for _ in 0..3 {
            let _ = mm.add_seek(500);
        }

        let summary = mm.list_seeks();
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].waiting, 3);
        assert_eq!(summary[0].max_points, 500);
    }

    #[tokio::test]
    async fn test_seek_different_max_points_no_match() {
        let mm = make_matchmaker();

        for _ in 0..3 {
            let _ = mm.add_seek(500);
        }
        let _ = mm.add_seek(300);

        let summary = mm.list_seeks();
        assert_eq!(summary.len(), 2);
    }

    #[tokio::test]
    async fn test_cancel_seek() {
        let mm = make_matchmaker();
        let player_id;
        {
            let _rx = mm.add_seek(500);
            let queue = mm.seek_queue.lock().unwrap();
            player_id = queue[0].player_id;
        }

        mm.cancel_seek(player_id);

        let summary = mm.list_seeks();
        assert_eq!(summary.len(), 0);
    }
}
