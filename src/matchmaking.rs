use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use tokio::sync::mpsc;
use crate::game_manager::GameManager;
use crate::{GameTransition, TimerConfig};

/// Result sent to matched players
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    pub game_id: Uuid,
    pub player_id: Uuid,
    pub player_short_id: String,
    pub player_url: String,
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

/// Summary of seeks waiting for a given max_points
#[derive(Debug, Serialize, Deserialize, oasgen::OaSchema)]
pub struct SeekSummary {
    pub max_points: i32,
    pub waiting: usize,
}

/// Queue size for a specific game configuration (max_points + timer_config).
#[derive(Debug, Serialize, Deserialize, oasgen::OaSchema)]
pub struct QueueSizeEntry {
    pub max_points: i32,
    pub timer_config: TimerConfig,
    pub waiting: usize,
}

struct PendingSeek {
    player_id: Uuid,
    max_points: i32,
    timer_config: TimerConfig,
    name: Option<String>,
    sender: mpsc::UnboundedSender<SeekEvent>,
}

/// Manages matchmaking: seek queue.
#[derive(Clone)]
pub struct Matchmaker {
    game_manager: GameManager,
    seek_queue: Arc<Mutex<Vec<PendingSeek>>>,
}

impl Matchmaker {
    pub fn new(game_manager: GameManager) -> Self {
        Matchmaker {
            game_manager,
            seek_queue: Arc::new(Mutex::new(Vec::new())),
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

    /// List queue sizes grouped by game configuration (max_points + timer_config).
    pub fn queue_sizes(&self) -> Vec<QueueSizeEntry> {
        let queue = self.seek_queue.lock().unwrap();
        let mut counts: HashMap<(i32, TimerConfig), usize> = HashMap::new();
        for seek in queue.iter() {
            *counts.entry((seek.max_points, seek.timer_config)).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .map(|((max_points, timer_config), waiting)| QueueSizeEntry {
                max_points,
                timer_config,
                waiting,
            })
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
                player_short_id: crate::uuid_to_short_id(seek.player_id),
                player_url: crate::encode_player_url(response.game_id, seek.player_id),
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
    async fn test_queue_sizes_groups_by_config() {
        let mm = make_matchmaker();
        let fast_timer = TimerConfig { initial_time_secs: 180, increment_secs: 2 };
        let slow_timer = TimerConfig { initial_time_secs: 600, increment_secs: 5 };

        // 2 seeks: 500 pts + fast timer
        let (_p1, _rx1) = mm.add_seek(500, fast_timer, None);
        let (_p2, _rx2) = mm.add_seek(500, fast_timer, None);
        // 1 seek: 500 pts + slow timer
        let (_p3, _rx3) = mm.add_seek(500, slow_timer, None);
        // 1 seek: 300 pts + fast timer
        let (_p4, _rx4) = mm.add_seek(300, fast_timer, None);

        let sizes = mm.queue_sizes();
        assert_eq!(sizes.len(), 3);

        let find = |mp: i32, tc: TimerConfig| sizes.iter().find(|e| e.max_points == mp && e.timer_config == tc);
        assert_eq!(find(500, fast_timer).unwrap().waiting, 2);
        assert_eq!(find(500, slow_timer).unwrap().waiting, 1);
        assert_eq!(find(300, fast_timer).unwrap().waiting, 1);
    }

}
