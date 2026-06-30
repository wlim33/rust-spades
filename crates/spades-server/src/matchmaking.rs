use crate::game_manager::GameManager;
use crate::lock_util::MutexExt;
use serde::{Deserialize, Serialize};
use spades::{GameTransition, TimerConfig};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use uuid::Uuid;

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

/// Summary of seeks waiting in a given rating band + max_points
#[derive(Debug, Serialize, Deserialize, oasgen::OaSchema)]
pub struct SeekSummary {
    pub band: u8,
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
    rating: f64,
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
    pub async fn add_seek(
        &self,
        max_points: i32,
        timer_config: TimerConfig,
        name: Option<String>,
        rating: f64,
    ) -> (Uuid, mpsc::UnboundedReceiver<SeekEvent>) {
        let player_id = Uuid::new_v4();
        let (tx, rx) = mpsc::unbounded_channel();

        {
            let mut queue = self.seek_queue.lock_or_recover();
            queue.push(PendingSeek {
                player_id,
                max_points,
                timer_config,
                rating,
                name,
                sender: tx,
            });
        }

        let band = crate::bands::band_of(rating);
        self.try_match(band, max_points, timer_config).await;
        self.notify_seekers(band, max_points, timer_config);
        (player_id, rx)
    }

    /// Remove a seek from the queue by player_id.
    pub fn cancel_seek(&self, player_id: Uuid) {
        let seek_info;
        {
            let mut queue = self.seek_queue.lock_or_recover();
            seek_info = queue
                .iter()
                .find(|s| s.player_id == player_id)
                .map(|s| (s.rating, s.max_points, s.timer_config));
            queue.retain(|s| s.player_id != player_id);
        }
        if let Some((rating, mp, tc)) = seek_info {
            self.notify_seekers(crate::bands::band_of(rating), mp, tc);
        }
    }

    /// List a summary of active seeks grouped by max_points.
    pub fn list_seeks(&self) -> Vec<SeekSummary> {
        let queue = self.seek_queue.lock_or_recover();
        let mut counts: HashMap<(u8, i32), usize> = HashMap::new();
        for seek in queue.iter() {
            *counts
                .entry((crate::bands::band_of(seek.rating), seek.max_points))
                .or_insert(0) += 1;
        }
        counts
            .into_iter()
            .map(|((band, max_points), waiting)| SeekSummary {
                band,
                max_points,
                waiting,
            })
            .collect()
    }

    /// List queue sizes grouped by game configuration (max_points + timer_config).
    pub fn queue_sizes(&self) -> Vec<QueueSizeEntry> {
        let queue = self.seek_queue.lock_or_recover();
        let mut counts: HashMap<(i32, TimerConfig), usize> = HashMap::new();
        for seek in queue.iter() {
            *counts
                .entry((seek.max_points, seek.timer_config))
                .or_insert(0) += 1;
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
    async fn try_match(&self, band: u8, max_points: i32, timer_config: TimerConfig) {
        // Scope the std::sync::MutexGuard tightly so it cannot leak into the
        // generated future state across any later `.await` — std::sync guards
        // are `!Send` and the handler must produce a `Send` future for axum.
        let seeks: Vec<PendingSeek> = {
            let mut queue = self.seek_queue.lock_or_recover();

            let matching: Vec<usize> = queue
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    crate::bands::band_of(s.rating) == band
                        && s.max_points == max_points
                        && s.timer_config == timer_config
                })
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
            seeks
        };

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
        let response = match self.game_manager.create_game_with_players(
            player_ids,
            max_points,
            Some(timer_config),
        ) {
            Ok(r) => r,
            Err(_) => {
                // Re-queue the seeks so players aren't lost
                let mut queue = self.seek_queue.lock_or_recover();
                for seek in seeks {
                    queue.push(seek);
                }
                return;
            }
        };

        if self
            .game_manager
            .make_transition(response.game_id, GameTransition::Start)
            .await
            .is_err()
        {
            // Game was created but couldn't start — remove it and re-queue seeks
            let _ = self.game_manager.remove_game(response.game_id);
            let mut queue = self.seek_queue.lock_or_recover();
            for seek in seeks {
                queue.push(seek);
            }
            return;
        }

        // Set player names on the game
        for (i, name) in player_names.iter().enumerate() {
            if name.is_some() {
                let _ = self
                    .game_manager
                    .set_player_name(response.game_id, player_ids[i], name.clone())
                    .await;
            }
        }

        // Notify all 4 players
        for seek in seeks {
            let result = MatchResult {
                game_id: response.game_id,
                player_id: seek.player_id,
                player_short_id: spades::uuid_to_short_id(seek.player_id),
                player_url: spades::encode_player_url(response.game_id, seek.player_id),
                player_ids,
                player_names: player_names.clone(),
                short_id: spades::uuid_to_short_id(response.game_id),
            };
            let _ = seek.sender.send(SeekEvent::GameStart(result));
        }
    }

    /// Notify all seekers in a given band + max_points + timer_config of the current queue count.
    fn notify_seekers(&self, band: u8, max_points: i32, timer_config: TimerConfig) {
        let queue = self.seek_queue.lock_or_recover();
        let matches = |s: &&PendingSeek| {
            crate::bands::band_of(s.rating) == band
                && s.max_points == max_points
                && s.timer_config == timer_config
        };
        let waiting = queue.iter().filter(matches).count();
        for seek in queue.iter().filter(matches) {
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
        TimerConfig {
            initial_time_secs: 300,
            increment_secs: 3,
        }
    }

    #[tokio::test]
    async fn test_seek_match_4_players() {
        let mm = make_matchmaker();
        let mut receivers = Vec::new();

        for _ in 0..4 {
            let (_pid, rx) = mm.add_seek(500, default_timer(), None, 1500.0).await;
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
            let _ = mm.add_seek(500, default_timer(), None, 1500.0).await;
        }

        let summary = mm.list_seeks();
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].waiting, 3);
        assert_eq!(summary[0].max_points, 500);
        assert_eq!(summary[0].band, 1); // all default 1500 -> Mid
    }

    #[tokio::test]
    async fn test_list_seeks_groups_by_band() {
        let mm = make_matchmaker();
        // 2 Mid + 1 Low waiting, none enough to match.
        let _ = mm.add_seek(500, default_timer(), None, 1500.0).await;
        let _ = mm.add_seek(500, default_timer(), None, 1550.0).await;
        let _ = mm.add_seek(500, default_timer(), None, 1200.0).await;

        let mut summary = mm.list_seeks();
        summary.sort_by_key(|s| s.band);
        assert_eq!(summary.len(), 2, "two bands occupied");
        assert_eq!(summary[0].band, 0); // Low
        assert_eq!(summary[0].waiting, 1);
        assert_eq!(summary[1].band, 1); // Mid
        assert_eq!(summary[1].waiting, 2);
    }

    #[tokio::test]
    async fn test_seek_different_max_points_no_match() {
        let mm = make_matchmaker();

        for _ in 0..3 {
            let _ = mm.add_seek(500, default_timer(), None, 1500.0).await;
        }
        let _ = mm.add_seek(300, default_timer(), None, 1500.0).await;

        let summary = mm.list_seeks();
        assert_eq!(summary.len(), 2);
    }

    #[tokio::test]
    async fn test_same_band_four_seekers_match() {
        let mm = make_matchmaker();
        let mut receivers = Vec::new();
        // Four Mid-band ratings (1400..1600) -> one game.
        for r in [1450.0, 1500.0, 1520.0, 1580.0] {
            let (_pid, rx) = mm.add_seek(500, default_timer(), None, r).await;
            receivers.push(rx);
        }
        let mut game_id = None;
        for mut rx in receivers {
            while let Some(event) = rx.recv().await {
                if let SeekEvent::GameStart(result) = event {
                    game_id = Some(result.game_id);
                    break;
                }
            }
        }
        assert!(game_id.is_some(), "four same-band seekers should match");
    }

    #[tokio::test]
    async fn test_cross_band_does_not_match() {
        let mm = make_matchmaker();
        // Three Mid + one High: different bands, so no game forms.
        for r in [1500.0, 1510.0, 1520.0] {
            let _ = mm.add_seek(500, default_timer(), None, r).await;
        }
        let _ = mm.add_seek(500, default_timer(), None, 1700.0).await; // High band
        // 4 seekers total but split 3 (Mid) + 1 (High): no match.
        let total: usize = mm.list_seeks().iter().map(|s| s.waiting).sum();
        assert_eq!(total, 4, "cross-band seeks must not be matched into a game");
    }

    #[tokio::test]
    async fn test_cancel_seek() {
        let mm = make_matchmaker();
        let (player_id, _rx) = mm.add_seek(500, default_timer(), None, 1500.0).await;

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
            let (_pid, rx) = mm
                .add_seek(500, default_timer(), Some(name.to_string()), 1500.0)
                .await;
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
        let (p1, _rx1) = mm.add_seek(500, default_timer(), None, 1500.0).await;
        let (_p2, _rx2) = mm.add_seek(500, default_timer(), None, 1500.0).await;
        let (_p3, _rx3) = mm.add_seek(500, default_timer(), None, 1500.0).await;

        mm.cancel_seek(p1);

        let summary = mm.list_seeks();
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].waiting, 2);
    }

    #[tokio::test]
    async fn test_queue_sizes_groups_by_config() {
        let mm = make_matchmaker();
        let fast_timer = TimerConfig {
            initial_time_secs: 180,
            increment_secs: 2,
        };
        let slow_timer = TimerConfig {
            initial_time_secs: 600,
            increment_secs: 5,
        };

        // 2 seeks: 500 pts + fast timer
        let (_p1, _rx1) = mm.add_seek(500, fast_timer, None, 1500.0).await;
        let (_p2, _rx2) = mm.add_seek(500, fast_timer, None, 1500.0).await;
        // 1 seek: 500 pts + slow timer
        let (_p3, _rx3) = mm.add_seek(500, slow_timer, None, 1500.0).await;
        // 1 seek: 300 pts + fast timer
        let (_p4, _rx4) = mm.add_seek(300, fast_timer, None, 1500.0).await;

        let sizes = mm.queue_sizes();
        assert_eq!(sizes.len(), 3);

        let find = |mp: i32, tc: TimerConfig| {
            sizes
                .iter()
                .find(|e| e.max_points == mp && e.timer_config == tc)
        };
        assert_eq!(find(500, fast_timer).unwrap().waiting, 2);
        assert_eq!(find(500, slow_timer).unwrap().waiting, 1);
        assert_eq!(find(300, fast_timer).unwrap().waiting, 1);
    }
}
