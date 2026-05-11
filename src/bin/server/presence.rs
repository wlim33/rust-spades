use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::dto::{PlayerPresenceEntry, PresenceSnapshot};

/// Per-game connection counts: game_id -> (player_id -> connection_count)
#[derive(Clone)]
pub struct PresenceTracker {
    /// game_id -> (player_id -> active connection count)
    connections: Arc<RwLock<HashMap<Uuid, HashMap<Uuid, usize>>>>,
    /// game_id -> broadcast sender for presence snapshots
    broadcasters: Arc<RwLock<HashMap<Uuid, broadcast::Sender<PresenceSnapshot>>>>,
}

impl PresenceTracker {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            broadcasters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Idempotent init for a game. Creates entry + broadcaster if missing.
    pub fn ensure_game(&self, game_id: Uuid, player_ids: &[Uuid]) {
        let mut conns = self.connections.write().unwrap();
        conns
            .entry(game_id)
            .or_insert_with(|| player_ids.iter().map(|&pid| (pid, 0usize)).collect());
        let mut bcast = self.broadcasters.write().unwrap();
        bcast
            .entry(game_id)
            .or_insert_with(|| broadcast::channel(16).0);
    }

    /// Increment connection count. Returns snapshot if player is in the game.
    pub fn player_connected(&self, game_id: Uuid, player_id: Uuid) -> Option<PresenceSnapshot> {
        let mut conns = self.connections.write().unwrap();
        let game = conns.get_mut(&game_id)?;
        let count = game.get_mut(&player_id)?;
        *count += 1;
        Some(Self::build_snapshot_from(game_id, game))
    }

    /// Decrement connection count (saturating). Returns snapshot if player is in the game.
    pub fn player_disconnected(&self, game_id: Uuid, player_id: Uuid) -> Option<PresenceSnapshot> {
        let mut conns = self.connections.write().unwrap();
        let game = conns.get_mut(&game_id)?;
        let count = game.get_mut(&player_id)?;
        *count = count.saturating_sub(1);
        Some(Self::build_snapshot_from(game_id, game))
    }

    /// Read-only current state.
    pub fn get_snapshot(&self, game_id: Uuid) -> Option<PresenceSnapshot> {
        let conns = self.connections.read().unwrap();
        let game = conns.get(&game_id)?;
        Some(Self::build_snapshot_from(game_id, game))
    }

    /// Subscribe to presence broadcasts for a game.
    pub fn subscribe(&self, game_id: Uuid) -> Option<broadcast::Receiver<PresenceSnapshot>> {
        let bcast = self.broadcasters.read().unwrap();
        bcast.get(&game_id).map(|tx| tx.subscribe())
    }

    /// Send snapshot to all subscribers.
    pub fn broadcast(&self, game_id: Uuid, snapshot: PresenceSnapshot) {
        let bcast = self.broadcasters.read().unwrap();
        if let Some(tx) = bcast.get(&game_id) {
            let _ = tx.send(snapshot);
        }
    }

    /// Clean up tracker state for a deleted game.
    pub fn remove_game(&self, game_id: Uuid) {
        self.connections.write().unwrap().remove(&game_id);
        self.broadcasters.write().unwrap().remove(&game_id);
    }

    fn build_snapshot_from(game_id: Uuid, game: &HashMap<Uuid, usize>) -> PresenceSnapshot {
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
