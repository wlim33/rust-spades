use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use tokio::sync::{mpsc, broadcast};
use crate::game_manager::GameManager;
use crate::{GameTransition, TimerConfig};

/// Result sent to matched players
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    pub game_id: Uuid,
    pub player_id: Uuid,
    pub player_ids: [Uuid; 4],
    pub player_names: [Option<String>; 4],
    pub short_id: String,
}

/// Event sent to seekers in the quickplay queue
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SeekEvent {
    QueueUpdate { waiting: usize },
    GameStart(MatchResult),
}

/// SSE event sent to lobby members
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LobbyEvent {
    LobbyUpdate { lobby_id: Uuid, players: usize, player_names: Vec<Option<String>> },
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
    pub player_names: Vec<Option<String>>,
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
    timer_config: TimerConfig,
    name: Option<String>,
    sender: mpsc::UnboundedSender<SeekEvent>,
}

struct LobbyPlayer {
    player_id: Uuid,
    name: Option<String>,
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

    /// Add a seek to the queue. Returns (player_id, receiver) so the caller
    /// can cancel the seek on disconnect.
    pub fn add_seek(&self, max_points: i32, timer_config: TimerConfig, name: Option<String>) -> (Uuid, mpsc::UnboundedReceiver<SeekEvent>) {
        let player_id = Uuid::new_v4();
        let (tx, rx) = mpsc::unbounded_channel();

        {
            let mut queue = self.seek_queue.lock().unwrap();
            queue.push(PendingSeek {
                player_id,
                max_points,
                timer_config,
                name,
                sender: tx,
            });
        }

        self.try_match(max_points, timer_config);
        self.notify_seekers(max_points, timer_config);
        (player_id, rx)
    }

    /// Remove a seek from the queue by player_id.
    pub fn cancel_seek(&self, player_id: Uuid) {
        let seek_info;
        {
            let mut queue = self.seek_queue.lock().unwrap();
            seek_info = queue.iter().find(|s| s.player_id == player_id).map(|s| (s.max_points, s.timer_config));
            queue.retain(|s| s.player_id != player_id);
        }
        if let Some((mp, tc)) = seek_info {
            self.notify_seekers(mp, tc);
        }
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

    /// Check if there are 4 seeks with the same max_points and timer_config and create a game.
    fn try_match(&self, max_points: i32, timer_config: TimerConfig) {
        let mut queue = self.seek_queue.lock().unwrap();

        let matching: Vec<usize> = queue
            .iter()
            .enumerate()
            .filter(|(_, s)| s.max_points == max_points && s.timer_config == timer_config)
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

        let player_names: [Option<String>; 4] = [
            seeks[0].name.clone(),
            seeks[1].name.clone(),
            seeks[2].name.clone(),
            seeks[3].name.clone(),
        ];

        // Create game with pre-assigned player IDs and auto-start
        let response = match self.game_manager.create_game_with_players(player_ids, max_points, Some(timer_config)) {
            Ok(r) => r,
            Err(_) => {
                // Re-queue the seeks so players aren't lost
                let mut queue = self.seek_queue.lock().unwrap();
                for seek in seeks {
                    queue.push(seek);
                }
                return;
            }
        };

        if self.game_manager.make_transition(response.game_id, GameTransition::Start).is_err() {
            // Game was created but couldn't start — remove it and re-queue seeks
            let _ = self.game_manager.remove_game(response.game_id);
            let mut queue = self.seek_queue.lock().unwrap();
            for seek in seeks {
                queue.push(seek);
            }
            return;
        }

        // Set player names on the game
        for (i, name) in player_names.iter().enumerate() {
            if name.is_some() {
                let _ = self.game_manager.set_player_name(response.game_id, player_ids[i], name.clone());
            }
        }

        // Notify all 4 players
        for seek in seeks {
            let result = MatchResult {
                game_id: response.game_id,
                player_id: seek.player_id,
                player_ids,
                player_names: player_names.clone(),
                short_id: crate::uuid_to_short_id(response.game_id),
            };
            let _ = seek.sender.send(SeekEvent::GameStart(result));
        }
    }

    /// Notify all seekers for a given max_points and timer_config of the current queue count.
    fn notify_seekers(&self, max_points: i32, timer_config: TimerConfig) {
        let queue = self.seek_queue.lock().unwrap();
        let matches = |s: &&PendingSeek| s.max_points == max_points && s.timer_config == timer_config;
        let waiting = queue.iter().filter(matches).count();
        for seek in queue.iter().filter(|s| s.max_points == max_points && s.timer_config == timer_config) {
            let _ = seek.sender.send(SeekEvent::QueueUpdate { waiting });
        }
    }

    /// Create a new lobby. Returns (lobby_id, creator_player_id, broadcast_receiver).
    pub fn create_lobby(&self, max_points: i32, name: Option<String>) -> (Uuid, Uuid, broadcast::Receiver<LobbyEvent>) {
        let lobby_id = Uuid::new_v4();
        let creator_id = Uuid::new_v4();
        let (broadcast_tx, broadcast_rx) = broadcast::channel(16);

        let lobby = Lobby {
            lobby_id,
            creator_id,
            max_points,
            players: vec![LobbyPlayer { player_id: creator_id, name }],
            broadcast_tx,
        };

        let mut lobbies = self.lobbies.write().unwrap();
        lobbies.insert(lobby_id, lobby);

        (lobby_id, creator_id, broadcast_rx)
    }

    /// Join an existing lobby. Returns (player_id, broadcast_receiver).
    pub fn join_lobby(&self, lobby_id: Uuid, name: Option<String>) -> Result<(Uuid, broadcast::Receiver<LobbyEvent>), MatchmakingError> {
        let mut lobbies = self.lobbies.write().map_err(|_| MatchmakingError::LockError)?;
        let lobby = lobbies.get_mut(&lobby_id).ok_or(MatchmakingError::LobbyNotFound)?;

        if lobby.players.len() >= 4 {
            return Err(MatchmakingError::LobbyFull);
        }

        let player_id = Uuid::new_v4();
        let rx = lobby.broadcast_tx.subscribe();
        lobby.players.push(LobbyPlayer { player_id, name });

        let player_count = lobby.players.len();
        let lobby_player_names: Vec<Option<String>> = lobby.players.iter().map(|p| p.name.clone()).collect();
        let _ = lobby.broadcast_tx.send(LobbyEvent::LobbyUpdate {
            lobby_id,
            players: player_count,
            player_names: lobby_player_names,
        });

        if player_count == 4 {
            let player_ids: [Uuid; 4] = [
                lobby.players[0].player_id,
                lobby.players[1].player_id,
                lobby.players[2].player_id,
                lobby.players[3].player_id,
            ];
            let player_names: [Option<String>; 4] = [
                lobby.players[0].name.clone(),
                lobby.players[1].name.clone(),
                lobby.players[2].name.clone(),
                lobby.players[3].name.clone(),
            ];
            let max_points = lobby.max_points;
            let broadcast_tx = lobby.broadcast_tx.clone();

            // Drop lock before calling game_manager
            drop(lobbies);

            match self.game_manager.create_game_with_players(player_ids, max_points, None) {
                Ok(response) => {
                    if self.game_manager.make_transition(response.game_id, GameTransition::Start).is_err() {
                        // Game was created but couldn't start — remove it, lobby stays
                        let _ = self.game_manager.remove_game(response.game_id);
                    } else {
                        // Set player names on the game
                        for (i, name) in player_names.iter().enumerate() {
                            if name.is_some() {
                                let _ = self.game_manager.set_player_name(response.game_id, player_ids[i], name.clone());
                            }
                        }

                        let _ = broadcast_tx.send(LobbyEvent::GameStart(MatchResult {
                            game_id: response.game_id,
                            player_id: Uuid::nil(),
                            player_ids,
                            player_names,
                            short_id: crate::uuid_to_short_id(response.game_id),
                        }));

                        // Remove lobby after successful game creation
                        let mut lobbies = self.lobbies.write().map_err(|_| MatchmakingError::LockError)?;
                        lobbies.remove(&lobby_id);
                    }
                }
                Err(_) => {
                    // Game creation failed — lobby stays, players remain
                }
            }
        }

        Ok((player_id, rx))
    }

    /// List open lobbies (not yet full).
    pub fn list_lobbies(&self) -> Vec<LobbySummary> {
        let lobbies = self.lobbies.read().unwrap();
        lobbies
            .values()
            .map(|l| LobbySummary {
                lobby_id: l.lobby_id,
                max_points: l.max_points,
                players: l.players.len(),
                player_names: l.players.iter().map(|p| p.name.clone()).collect(),
            })
            .collect()
    }

    /// Delete a lobby. Only the creator can delete it.
    pub fn delete_lobby(&self, lobby_id: Uuid, requester_id: Uuid) -> Result<(), MatchmakingError> {
        let mut lobbies = self.lobbies.write().map_err(|_| MatchmakingError::LockError)?;
        let lobby = lobbies.get(&lobby_id).ok_or(MatchmakingError::LobbyNotFound)?;
        if lobby.creator_id != requester_id {
            return Err(MatchmakingError::LobbyNotFound);
        }
        lobbies.remove(&lobby_id);
        Ok(())
    }

    /// Remove a player from a lobby. If the creator leaves, the lobby is deleted.
    pub fn leave_lobby(&self, lobby_id: Uuid, player_id: Uuid) {
        let mut lobbies = self.lobbies.write().unwrap();
        if let Some(lobby) = lobbies.get_mut(&lobby_id) {
            if lobby.creator_id == player_id {
                lobbies.remove(&lobby_id);
            } else {
                lobby.players.retain(|p| p.player_id != player_id);
                let player_count = lobby.players.len();
                let lobby_player_names: Vec<Option<String>> = lobby.players.iter().map(|p| p.name.clone()).collect();
                let _ = lobby.broadcast_tx.send(LobbyEvent::LobbyUpdate {
                    lobby_id,
                    players: player_count,
                    player_names: lobby_player_names,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_matchmaker() -> Matchmaker {
        Matchmaker::new(GameManager::new())
    }

    fn default_timer() -> TimerConfig {
        TimerConfig { initial_time_secs: 300, increment_secs: 3 }
    }

    #[tokio::test]
    async fn test_seek_match_4_players() {
        let mm = make_matchmaker();
        let mut receivers = Vec::new();

        for _ in 0..4 {
            let (_pid, rx) = mm.add_seek(500, default_timer(), None);
            receivers.push(rx);
        }

        let mut game_id = None;
        for mut rx in receivers {
            // Drain until we get GameStart
            while let Some(event) = rx.recv().await {
                if let SeekEvent::GameStart(result) = event {
                    if let Some(gid) = game_id {
                        assert_eq!(result.game_id, gid, "all players should be in same game");
                    } else {
                        game_id = Some(result.game_id);
                    }
                    assert_eq!(result.player_ids.len(), 4);
                    break;
                }
            }
        }
        assert!(game_id.is_some());
    }

    #[tokio::test]
    async fn test_seek_no_match_with_3_players() {
        let mm = make_matchmaker();

        for _ in 0..3 {
            let _ = mm.add_seek(500, default_timer(), None);
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
            let _ = mm.add_seek(500, default_timer(), None);
        }
        let _ = mm.add_seek(300, default_timer(), None);

        let summary = mm.list_seeks();
        assert_eq!(summary.len(), 2);
    }

    #[tokio::test]
    async fn test_cancel_seek() {
        let mm = make_matchmaker();
        let (player_id, _rx) = mm.add_seek(500, default_timer(), None);

        mm.cancel_seek(player_id);

        let summary = mm.list_seeks();
        assert_eq!(summary.len(), 0);
    }

    #[tokio::test]
    async fn test_create_lobby() {
        let mm = make_matchmaker();
        let (lobby_id, _player_id, _rx) = mm.create_lobby(500, None);

        let lobbies = mm.list_lobbies();
        assert_eq!(lobbies.len(), 1);
        assert_eq!(lobbies[0].lobby_id, lobby_id);
        assert_eq!(lobbies[0].max_points, 500);
        assert_eq!(lobbies[0].players, 1);
    }

    #[tokio::test]
    async fn test_join_lobby() {
        let mm = make_matchmaker();
        let (lobby_id, _creator_id, _creator_rx) = mm.create_lobby(500, None);

        let result = mm.join_lobby(lobby_id, None);
        assert!(result.is_ok());

        let lobbies = mm.list_lobbies();
        assert_eq!(lobbies[0].players, 2);
    }

    #[tokio::test]
    async fn test_join_lobby_not_found() {
        let mm = make_matchmaker();
        let result = mm.join_lobby(Uuid::new_v4(), None);
        assert!(matches!(result, Err(MatchmakingError::LobbyNotFound)));
    }

    #[tokio::test]
    async fn test_lobby_auto_start_on_4th_player() {
        let mm = make_matchmaker();
        let (lobby_id, _creator_id, mut creator_rx) = mm.create_lobby(500, None);

        let mut _join_rxs = Vec::new();
        for _ in 0..3 {
            let (_player_id, rx) = mm.join_lobby(lobby_id, None).unwrap();
            _join_rxs.push(rx);
        }

        // Creator should have received game_start via broadcast
        // The first event might be a LobbyUpdate, so loop until we get GameStart
        loop {
            let event = creator_rx.recv().await.unwrap();
            match event {
                LobbyEvent::GameStart(result) => {
                    assert_eq!(result.player_ids.len(), 4);
                    break;
                }
                LobbyEvent::LobbyUpdate { .. } => continue,
            }
        }

        // Lobby should be removed after game starts
        let lobbies = mm.list_lobbies();
        assert_eq!(lobbies.len(), 0);
    }

    #[tokio::test]
    async fn test_delete_lobby() {
        let mm = make_matchmaker();
        let (lobby_id, creator_id, _rx) = mm.create_lobby(500, None);

        let result = mm.delete_lobby(lobby_id, creator_id);
        assert!(result.is_ok());

        let lobbies = mm.list_lobbies();
        assert_eq!(lobbies.len(), 0);
    }

    #[tokio::test]
    async fn test_leave_lobby() {
        let mm = make_matchmaker();
        let (lobby_id, _creator_id, _creator_rx) = mm.create_lobby(500, None);
        let (joiner_id, _rx) = mm.join_lobby(lobby_id, None).unwrap();

        mm.leave_lobby(lobby_id, joiner_id);

        let lobbies = mm.list_lobbies();
        assert_eq!(lobbies[0].players, 1);
    }

    #[tokio::test]
    async fn test_seek_with_names_propagated() {
        let mm = make_matchmaker();
        let mut receivers = Vec::new();
        let names = ["Alice", "Bob", "Carol", "Dave"];

        for name in &names {
            let (_pid, rx) = mm.add_seek(500, default_timer(), Some(name.to_string()));
            receivers.push(rx);
        }

        for mut rx in receivers {
            while let Some(event) = rx.recv().await {
                if let SeekEvent::GameStart(result) = event {
                    // All 4 player_names should be Some
                    for pn in &result.player_names {
                        assert!(pn.is_some());
                    }
                    break;
                }
            }
        }
    }

    #[tokio::test]
    async fn test_lobby_creator_leaves_deletes_lobby() {
        let mm = make_matchmaker();
        let (lobby_id, creator_id, _rx) = mm.create_lobby(500, None);
        let _ = mm.join_lobby(lobby_id, None).unwrap();

        // Creator leaving deletes the lobby
        mm.leave_lobby(lobby_id, creator_id);

        let lobbies = mm.list_lobbies();
        assert_eq!(lobbies.len(), 0);
    }

    #[tokio::test]
    async fn test_lobby_with_player_names() {
        let mm = make_matchmaker();
        let (_lobby_id, _creator_id, _rx) = mm.create_lobby(500, Some("Alice".to_string()));

        let lobbies = mm.list_lobbies();
        assert_eq!(lobbies[0].player_names[0].as_deref(), Some("Alice"));
    }

    #[tokio::test]
    async fn test_leave_lobby_broadcasts_update() {
        let mm = make_matchmaker();
        let (lobby_id, _creator_id, mut creator_rx) = mm.create_lobby(500, None);
        let (joiner_id, _joiner_rx) = mm.join_lobby(lobby_id, None).unwrap();

        // Drain the LobbyUpdate from join
        let _ = creator_rx.try_recv();

        mm.leave_lobby(lobby_id, joiner_id);

        // Should receive a LobbyUpdate with 1 player
        let event = creator_rx.try_recv().unwrap();
        match event {
            LobbyEvent::LobbyUpdate { players, .. } => assert_eq!(players, 1),
            _ => panic!("Expected LobbyUpdate"),
        }
    }

    #[tokio::test]
    async fn test_seek_list_after_partial_cancel() {
        let mm = make_matchmaker();
        let (p1, _rx1) = mm.add_seek(500, default_timer(), None);
        let (_p2, _rx2) = mm.add_seek(500, default_timer(), None);
        let (_p3, _rx3) = mm.add_seek(500, default_timer(), None);

        mm.cancel_seek(p1);

        let summary = mm.list_seeks();
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].waiting, 2);
    }

    #[tokio::test]
    async fn test_join_full_lobby() {
        let mm = make_matchmaker();
        let (lobby_id, _creator_id, _rx) = mm.create_lobby(500, None);

        // Fill up the lobby to 4 players
        for _ in 0..3 {
            let _ = mm.join_lobby(lobby_id, None).unwrap();
        }

        // Lobby should be gone (game started), so joining returns LobbyNotFound
        let result = mm.join_lobby(lobby_id, None);
        assert!(matches!(result, Err(MatchmakingError::LobbyNotFound)));
    }

    #[tokio::test]
    async fn test_delete_lobby_wrong_creator() {
        let mm = make_matchmaker();
        let (lobby_id, _creator_id, _rx) = mm.create_lobby(500, None);

        let result = mm.delete_lobby(lobby_id, Uuid::new_v4());
        assert!(matches!(result, Err(MatchmakingError::LobbyNotFound)));
    }
}
