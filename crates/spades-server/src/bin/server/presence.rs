use spades_server::lock_util::RwLockExt;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::dto::{PlayerPresenceEntry, PresenceSnapshot};

/// Per-game connection counts: game_id -> (player_id -> connection_count).
/// Spectators (WS subscribers without a `player_id`) are tracked separately
/// in `spectators` since they have no seat to associate with.
#[derive(Clone)]
pub struct PresenceTracker {
    /// game_id -> (player_id -> active connection count)
    connections: Arc<RwLock<HashMap<Uuid, HashMap<Uuid, usize>>>>,
    /// game_id -> count of WS subscribers without a player_id
    spectators: Arc<RwLock<HashMap<Uuid, usize>>>,
    /// game_id -> broadcast sender for presence snapshots
    broadcasters: Arc<RwLock<HashMap<Uuid, broadcast::Sender<PresenceSnapshot>>>>,
}

impl PresenceTracker {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            spectators: Arc::new(RwLock::new(HashMap::new())),
            broadcasters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Idempotent init for a game. Creates entry + broadcaster if missing.
    pub fn ensure_game(&self, game_id: Uuid, player_ids: &[Uuid]) {
        let mut conns = self.connections.write_or_recover();
        conns
            .entry(game_id)
            .or_insert_with(|| player_ids.iter().map(|&pid| (pid, 0usize)).collect());
        self.spectators
            .write_or_recover()
            .entry(game_id)
            .or_insert(0);
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
        Some(self.build_snapshot(game_id, &players))
    }

    /// Decrement connection count (saturating). Returns snapshot if player is in the game.
    pub fn player_disconnected(&self, game_id: Uuid, player_id: Uuid) -> Option<PresenceSnapshot> {
        let mut conns = self.connections.write_or_recover();
        let game = conns.get_mut(&game_id)?;
        let count = game.get_mut(&player_id)?;
        *count = count.saturating_sub(1);
        let players = game.clone();
        drop(conns);
        Some(self.build_snapshot(game_id, &players))
    }

    /// Increment the spectator count for `game_id`. Returns the updated
    /// snapshot, or `None` if the game has never been initialised.
    pub fn spectator_connected(&self, game_id: Uuid) -> Option<PresenceSnapshot> {
        let mut specs = self.spectators.write_or_recover();
        let entry = specs.get_mut(&game_id)?;
        *entry += 1;
        drop(specs);
        let players = self.connections.read_or_recover().get(&game_id).cloned()?;
        Some(self.build_snapshot(game_id, &players))
    }

    /// Saturating decrement of the spectator count.
    pub fn spectator_disconnected(&self, game_id: Uuid) -> Option<PresenceSnapshot> {
        let mut specs = self.spectators.write_or_recover();
        let entry = specs.get_mut(&game_id)?;
        *entry = entry.saturating_sub(1);
        drop(specs);
        let players = self.connections.read_or_recover().get(&game_id).cloned()?;
        Some(self.build_snapshot(game_id, &players))
    }

    /// Read-only current state.
    pub fn get_snapshot(&self, game_id: Uuid) -> Option<PresenceSnapshot> {
        let conns = self.connections.read_or_recover();
        let game = conns.get(&game_id)?.clone();
        drop(conns);
        Some(self.build_snapshot(game_id, &game))
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
        self.spectators.write_or_recover().remove(&game_id);
        self.broadcasters.write_or_recover().remove(&game_id);
    }

    fn build_snapshot(&self, game_id: Uuid, players: &HashMap<Uuid, usize>) -> PresenceSnapshot {
        let spectator_count = self
            .spectators
            .read_or_recover()
            .get(&game_id)
            .copied()
            .unwrap_or(0);
        PresenceSnapshot {
            game_id,
            players: players
                .iter()
                .map(|(&pid, &count)| PlayerPresenceEntry {
                    player_id: pid,
                    connected: count > 0,
                })
                .collect(),
            spectator_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectator_count_increments_and_saturates() {
        let tracker = PresenceTracker::new();
        let game_id = Uuid::new_v4();
        let players = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];
        tracker.ensure_game(game_id, &players);

        let snap = tracker
            .spectator_connected(game_id)
            .expect("game initialised");
        assert_eq!(snap.spectator_count, 1);
        let snap = tracker.spectator_connected(game_id).unwrap();
        assert_eq!(snap.spectator_count, 2);
        let snap = tracker.spectator_disconnected(game_id).unwrap();
        assert_eq!(snap.spectator_count, 1);
        let snap = tracker.spectator_disconnected(game_id).unwrap();
        assert_eq!(snap.spectator_count, 0);
        // Saturating: another disconnect from zero stays at zero.
        let snap = tracker.spectator_disconnected(game_id).unwrap();
        assert_eq!(snap.spectator_count, 0);
    }

    #[test]
    fn spectator_methods_return_none_for_uninit_game() {
        let tracker = PresenceTracker::new();
        let game_id = Uuid::new_v4();
        assert!(tracker.spectator_connected(game_id).is_none());
        assert!(tracker.spectator_disconnected(game_id).is_none());
    }

    #[test]
    fn snapshot_includes_both_players_and_spectator_count() {
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
        tracker.spectator_connected(game_id);
        let snap = tracker.get_snapshot(game_id).unwrap();
        assert_eq!(snap.spectator_count, 1);
        // One of four players is connected.
        let connected_count = snap.players.iter().filter(|p| p.connected).count();
        assert_eq!(connected_count, 1);
    }
}
