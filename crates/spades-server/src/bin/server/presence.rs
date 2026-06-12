use spades_server::lock_util::RwLockExt;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::dto::{PlayerPresenceEntry, PresenceSnapshot};

/// Per-game connection counts: game_id -> (player_id -> connection_count).
/// Every WS subscriber is a seat owner (the upgrade is authorized in
/// `game_ws`), so there is no spectator tracking — the wire-level
/// `spectator_count` is kept for client compatibility and is always 0.
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
        let mut conns = self.connections.write_or_recover();
        conns
            .entry(game_id)
            .or_insert_with(|| player_ids.iter().map(|&pid| (pid, 0usize)).collect());
        let mut bcast = self.broadcasters.write_or_recover();
        bcast
            .entry(game_id)
            .or_insert_with(|| broadcast::channel(16).0);
    }

    /// Increment connection count. Returns snapshot if player is in the game.
    pub fn player_connected(&self, game_id: Uuid, player_id: Uuid) -> Option<PresenceSnapshot> {
        let mut conns = self.connections.write_or_recover();
        let game = conns.get_mut(&game_id)?;
        let count = game.get_mut(&player_id)?;
        *count += 1;
        let players = game.clone();
        drop(conns);
        Some(build_snapshot(game_id, &players))
    }

    /// Decrement connection count (saturating). Returns snapshot if player is in the game.
    pub fn player_disconnected(&self, game_id: Uuid, player_id: Uuid) -> Option<PresenceSnapshot> {
        let mut conns = self.connections.write_or_recover();
        let game = conns.get_mut(&game_id)?;
        let count = game.get_mut(&player_id)?;
        *count = count.saturating_sub(1);
        let players = game.clone();
        drop(conns);
        Some(build_snapshot(game_id, &players))
    }

    /// Read-only current state.
    pub fn get_snapshot(&self, game_id: Uuid) -> Option<PresenceSnapshot> {
        let conns = self.connections.read_or_recover();
        let game = conns.get(&game_id)?.clone();
        drop(conns);
        Some(build_snapshot(game_id, &game))
    }

    /// Subscribe to presence broadcasts for a game.
    pub fn subscribe(&self, game_id: Uuid) -> Option<broadcast::Receiver<PresenceSnapshot>> {
        let bcast = self.broadcasters.read_or_recover();
        bcast.get(&game_id).map(|tx| tx.subscribe())
    }

    /// Send snapshot to all subscribers.
    pub fn broadcast(&self, game_id: Uuid, snapshot: PresenceSnapshot) {
        let bcast = self.broadcasters.read_or_recover();
        if let Some(tx) = bcast.get(&game_id) {
            let _ = tx.send(snapshot);
        }
    }

    /// Clean up tracker state for a deleted game.
    pub fn remove_game(&self, game_id: Uuid) {
        self.connections.write_or_recover().remove(&game_id);
        self.broadcasters.write_or_recover().remove(&game_id);
    }
}

fn build_snapshot(game_id: Uuid, players: &HashMap<Uuid, usize>) -> PresenceSnapshot {
    PresenceSnapshot {
        game_id,
        players: players
            .iter()
            .map(|(&pid, &count)| PlayerPresenceEntry {
                player_id: pid,
                connected: count > 0,
            })
            .collect(),
        // Spectator WS connections were removed when the stream became
        // private to seat owners; the field stays so existing clients keep
        // parsing presence events.
        spectator_count: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reports_connected_players_and_zero_spectators() {
        let tracker = PresenceTracker::new();
        let game_id = Uuid::new_v4();
        let players = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];
        tracker.ensure_game(game_id, &players);
        tracker.player_connected(game_id, players[0]);
        let snap = tracker.get_snapshot(game_id).unwrap();
        // Game streams are private: spectator_count is wire-compat only.
        assert_eq!(snap.spectator_count, 0);
        // One of four players is connected.
        let connected_count = snap.players.iter().filter(|p| p.connected).count();
        assert_eq!(connected_count, 1);
    }
}
