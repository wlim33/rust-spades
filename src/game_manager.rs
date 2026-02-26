use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use tokio::sync::broadcast;
use crate::{Game, GameTransition, State, Card, TimerConfig};
use crate::result::TransitionSuccess;
use crate::sqlite_store::SqliteStore;
use serde::{Serialize, Deserialize};
use rand::seq::SliceRandom;

/// Event broadcast to WebSocket subscribers when game state changes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GameEvent {
    StateChanged(GameStateResponse),
    GameAborted { game_id: Uuid, reason: String },
}

/// Runtime-only timer state for an active turn (not serialized)
struct ActiveTurnTimer {
    turn_started_at: tokio::time::Instant,
    remaining_at_turn_start_ms: u64,
    timeout_handle: tokio::task::JoinHandle<()>,
    expected_player_index: usize,
}

/// Manages multiple concurrent spades games
#[derive(Clone)]
pub struct GameManager {
    games: Arc<RwLock<HashMap<Uuid, Arc<RwLock<Game>>>>>,
    broadcasters: Arc<RwLock<HashMap<Uuid, broadcast::Sender<GameEvent>>>>,
    db: Option<Arc<SqliteStore>>,
    active_timers: Arc<Mutex<HashMap<Uuid, ActiveTurnTimer>>>,
}

/// Response for creating a new game
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateGameResponse {
    pub game_id: Uuid,
    pub player_ids: [Uuid; 4],
}

/// A player's name entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerNameEntry {
    pub player_id: Uuid,
    pub name: Option<String>,
}

/// Response for getting game state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStateResponse {
    pub game_id: Uuid,
    pub state: State,
    pub team_a_score: Option<i32>,
    pub team_b_score: Option<i32>,
    pub team_a_bags: Option<i32>,
    pub team_b_bags: Option<i32>,
    pub current_player_id: Option<Uuid>,
    pub player_names: [PlayerNameEntry; 4],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timer_config: Option<TimerConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_clocks_ms: Option<[u64; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_player_clock_ms: Option<u64>,
}

/// Response for getting a player's hand
#[derive(Debug, Serialize, Deserialize)]
pub struct HandResponse {
    pub player_id: Uuid,
    pub cards: Vec<Card>,
}

/// Request to make a game transition
#[derive(Debug, Serialize, Deserialize)]
pub struct TransitionRequest {
    pub game_id: Uuid,
    pub player_id: Uuid,
    #[serde(flatten)]
    pub transition: TransitionRequestType,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransitionRequestType {
    Start,
    Bet { amount: i32 },
    Card { card: Card },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum GameManagerError {
    GameNotFound,
    GameError(String),
    LockError,
}

fn epoch_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl GameManager {
    /// Create a new game manager (in-memory only)
    pub fn new() -> Self {
        GameManager {
            games: Arc::new(RwLock::new(HashMap::new())),
            broadcasters: Arc::new(RwLock::new(HashMap::new())),
            db: None,
            active_timers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a game manager backed by SQLite. Loads existing games from the database.
    pub fn with_db(path: &str) -> Result<Self, String> {
        let store = SqliteStore::open(path)?;
        let existing_games = store.load_all_games()?;
        let mut games_map = HashMap::new();
        let mut broadcasters_map = HashMap::new();
        for game in existing_games {
            let id = *game.get_id();
            games_map.insert(id, Arc::new(RwLock::new(game)));
            let (tx, _) = broadcast::channel(64);
            broadcasters_map.insert(id, tx);
        }
        let count = games_map.len();
        println!("Loaded {} game(s) from database", count);
        let manager = GameManager {
            games: Arc::new(RwLock::new(games_map)),
            broadcasters: Arc::new(RwLock::new(broadcasters_map)),
            db: Some(Arc::new(store)),
            active_timers: Arc::new(Mutex::new(HashMap::new())),
        };

        // Restart timers for in-progress timed games
        manager.restart_persisted_timers();

        Ok(manager)
    }

    /// Restart timers for timed games that were in-progress when the server stopped.
    fn restart_persisted_timers(&self) {
        let games = match self.games.read() {
            Ok(g) => g,
            Err(_) => return,
        };
        for (&game_id, game_lock) in games.iter() {
            let game = match game_lock.read() {
                Ok(g) => g,
                Err(_) => continue,
            };
            if game.get_timer_config().is_none() {
                continue;
            }
            match game.get_state() {
                State::Betting(_) | State::Trick(_) => {}
                _ => continue,
            }
            if let (Some(epoch_ms), Some(clocks)) = (game.get_turn_started_at_epoch_ms(), game.get_player_clocks()) {
                let player_idx = game.get_current_player_index_num();
                let now = epoch_ms_now();
                let elapsed = now.saturating_sub(epoch_ms);
                let remaining = clocks.remaining_ms[player_idx].saturating_sub(elapsed);
                self.start_turn_timer(game_id, player_idx, remaining);
            }
        }
    }

    fn persist_insert(&self, game: &Game) {
        if let Some(db) = &self.db {
            if let Err(e) = db.insert_game(game) {
                eprintln!("Failed to persist game insert: {}", e);
            }
        }
    }

    fn persist_update(&self, game: &Game) {
        if let Some(db) = &self.db {
            if let Err(e) = db.update_game(game) {
                eprintln!("Failed to persist game update: {}", e);
            }
        }
    }

    fn persist_delete(&self, game_id: Uuid) {
        if let Some(db) = &self.db {
            if let Err(e) = db.delete_game(game_id) {
                eprintln!("Failed to persist game delete: {}", e);
            }
        }
    }

    /// Create a new game with 4 players
    pub fn create_game(&self, max_points: i32, timer_config: Option<TimerConfig>) -> Result<CreateGameResponse, GameManagerError> {
        let game_id = Uuid::new_v4();
        let player_ids = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];

        let game = Game::new(game_id, player_ids, max_points, timer_config);
        self.persist_insert(&game);

        let mut games = self.games.write().map_err(|_| GameManagerError::LockError)?;
        games.insert(game_id, Arc::new(RwLock::new(game)));
        drop(games);

        let (tx, _) = broadcast::channel(64);
        let mut broadcasters = self.broadcasters.write().map_err(|_| GameManagerError::LockError)?;
        broadcasters.insert(game_id, tx);

        Ok(CreateGameResponse {
            game_id,
            player_ids,
        })
    }

    /// Create a new game with pre-assigned player IDs
    pub fn create_game_with_players(&self, player_ids: [Uuid; 4], max_points: i32, timer_config: Option<TimerConfig>) -> Result<CreateGameResponse, GameManagerError> {
        let game_id = Uuid::new_v4();
        let game = Game::new(game_id, player_ids, max_points, timer_config);
        self.persist_insert(&game);

        let mut games = self.games.write().map_err(|_| GameManagerError::LockError)?;
        games.insert(game_id, Arc::new(RwLock::new(game)));
        drop(games);

        let (tx, _) = broadcast::channel(64);
        let mut broadcasters = self.broadcasters.write().map_err(|_| GameManagerError::LockError)?;
        broadcasters.insert(game_id, tx);

        Ok(CreateGameResponse {
            game_id,
            player_ids,
        })
    }

    fn build_state_response(game_id: Uuid, game: &Game, active_timer: Option<&ActiveTurnTimer>) -> GameStateResponse {
        let names = game.get_player_names();
        let timer_config = game.get_timer_config().copied();

        let (player_clocks_ms, active_player_clock_ms) = if let Some(clocks) = game.get_player_clocks() {
            let mut clocks_snapshot = clocks.remaining_ms;
            let mut active_clock = None;

            if let Some(timer) = active_timer {
                let elapsed = timer.turn_started_at.elapsed().as_millis() as u64;
                let remaining = timer.remaining_at_turn_start_ms.saturating_sub(elapsed);
                clocks_snapshot[timer.expected_player_index] = remaining;
                active_clock = Some(remaining);
            }

            (Some(clocks_snapshot), active_clock)
        } else {
            (None, None)
        };

        GameStateResponse {
            game_id,
            state: game.get_state().clone(),
            team_a_score: game.get_team_a_score().ok().copied(),
            team_b_score: game.get_team_b_score().ok().copied(),
            team_a_bags: game.get_team_a_bags().ok().copied(),
            team_b_bags: game.get_team_b_bags().ok().copied(),
            current_player_id: game.get_current_player_id().ok().copied(),
            player_names: [
                PlayerNameEntry { player_id: names[0].0, name: names[0].1.map(String::from) },
                PlayerNameEntry { player_id: names[1].0, name: names[1].1.map(String::from) },
                PlayerNameEntry { player_id: names[2].0, name: names[2].1.map(String::from) },
                PlayerNameEntry { player_id: names[3].0, name: names[3].1.map(String::from) },
            ],
            timer_config,
            player_clocks_ms,
            active_player_clock_ms,
        }
    }

    /// Build state response with real-time timer data
    pub fn build_state_response_with_timer(&self, game_id: Uuid, game: &Game) -> GameStateResponse {
        let timers = self.active_timers.lock().unwrap();
        let active_timer = timers.get(&game_id);
        Self::build_state_response(game_id, game, active_timer)
    }

    /// Get the state of a game
    pub fn get_game_state(&self, game_id: Uuid) -> Result<GameStateResponse, GameManagerError> {
        let games = self.games.read().map_err(|_| GameManagerError::LockError)?;
        let game_lock = games.get(&game_id).ok_or(GameManagerError::GameNotFound)?;
        let game = game_lock.read().map_err(|_| GameManagerError::LockError)?;

        let timers = self.active_timers.lock().map_err(|_| GameManagerError::LockError)?;
        let active_timer = timers.get(&game_id);
        Ok(Self::build_state_response(game_id, &game, active_timer))
    }

    /// Get a player's hand
    pub fn get_hand(&self, game_id: Uuid, player_id: Uuid) -> Result<HandResponse, GameManagerError> {
        let games = self.games.read().map_err(|_| GameManagerError::LockError)?;
        let game_lock = games.get(&game_id).ok_or(GameManagerError::GameNotFound)?;
        let game = game_lock.read().map_err(|_| GameManagerError::LockError)?;

        let cards = game.get_hand_by_player_id(player_id)
            .map_err(|e| GameManagerError::GameError(format!("{:?}", e)))?
            .clone();

        Ok(HandResponse {
            player_id,
            cards,
        })
    }

    /// Cancel the active turn timer for a game.
    /// Returns (elapsed_ms, previous_player_index).
    fn cancel_turn_timer(&self, game_id: Uuid) -> (u64, Option<usize>) {
        let mut timers = self.active_timers.lock().unwrap();
        if let Some(timer) = timers.remove(&game_id) {
            timer.timeout_handle.abort();
            let elapsed = timer.turn_started_at.elapsed().as_millis() as u64;
            (elapsed, Some(timer.expected_player_index))
        } else {
            (0, None)
        }
    }

    /// Start a turn timer for the given player. Spawns a tokio task that fires on timeout.
    fn start_turn_timer(&self, game_id: Uuid, player_index: usize, remaining_ms: u64) {
        let mgr = self.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(remaining_ms)).await;
            mgr.handle_timeout(game_id);
        });

        let mut timers = self.active_timers.lock().unwrap();
        timers.insert(game_id, ActiveTurnTimer {
            turn_started_at: tokio::time::Instant::now(),
            remaining_at_turn_start_ms: remaining_ms,
            timeout_handle: handle,
            expected_player_index: player_index,
        });
    }

    /// Make a game transition (start, bet, play card) — public entry point.
    /// For timed games, use `make_move()` instead to handle timer lifecycle.
    pub fn make_transition(&self, game_id: Uuid, transition: GameTransition)
        -> Result<TransitionSuccess, GameManagerError> {
        self.make_transition_internal(game_id, transition, false)
    }

    /// Internal transition handler with timeout awareness.
    fn make_transition_internal(&self, game_id: Uuid, transition: GameTransition, is_timeout: bool)
        -> Result<TransitionSuccess, GameManagerError> {
        let games = self.games.read().map_err(|_| GameManagerError::LockError)?;
        let game_lock = games.get(&game_id).ok_or(GameManagerError::GameNotFound)?;
        let mut game = game_lock.write().map_err(|_| GameManagerError::LockError)?;

        let is_timed = game.get_timer_config().is_some();
        let is_start = matches!(transition, GameTransition::Start);

        // Cancel existing timer and update clock before the move
        if is_timed && !is_start {
            let (elapsed_ms, prev_idx) = self.cancel_turn_timer(game_id);
            let increment_ms = if !is_timeout {
                game.get_timer_config().map(|tc| tc.increment_secs * 1000).unwrap_or(0)
            } else {
                0
            };
            if let Some(idx) = prev_idx {
                if let Some(clocks) = game.get_player_clocks_mut() {
                    clocks.remaining_ms[idx] = clocks.remaining_ms[idx].saturating_sub(elapsed_ms) + increment_ms;
                }
            }
        }

        let result = game.play(transition)
            .map_err(|e| GameManagerError::GameError(format!("{:?}", e)))?;

        // Set epoch timestamp for persistence/recovery
        if is_timed {
            match game.get_state() {
                State::Betting(_) | State::Trick(_) => {
                    game.set_turn_started_at_epoch_ms(Some(epoch_ms_now()));
                }
                _ => {
                    game.set_turn_started_at_epoch_ms(None);
                }
            }
        }

        self.persist_update(&game);

        // Start timer for the next player if game is still active
        if is_timed {
            match game.get_state() {
                State::Betting(_) | State::Trick(_) => {
                    let player_idx = game.get_current_player_index_num();
                    let remaining = game.get_player_clocks()
                        .map(|c| c.remaining_ms[player_idx])
                        .unwrap_or(0);
                    self.start_turn_timer(game_id, player_idx, remaining);
                }
                _ => {}
            }
        }

        let timers = self.active_timers.lock().unwrap();
        let active_timer = timers.get(&game_id);
        let state_response = Self::build_state_response(game_id, &game, active_timer);
        drop(timers);

        drop(game);
        drop(games);

        if let Ok(broadcasters) = self.broadcasters.read() {
            if let Some(tx) = broadcasters.get(&game_id) {
                let _ = tx.send(GameEvent::StateChanged(state_response));
            }
        }

        Ok(result)
    }

    /// Handle a timeout for the current player (called from spawned timer task).
    /// This is entirely synchronous to avoid recursive async Send issues.
    fn handle_timeout(&self, game_id: Uuid) {
        // Read game state to determine what to do
        let (is_first_round_betting, current_state, player_idx) = {
            let games = match self.games.read() {
                Ok(g) => g,
                Err(_) => return,
            };
            let game_lock = match games.get(&game_id) {
                Some(g) => g,
                None => return,
            };
            let game = match game_lock.read() {
                Ok(g) => g,
                Err(_) => return,
            };
            (
                game.is_first_round_betting(),
                game.get_state().clone(),
                game.get_current_player_index_num(),
            )
        };

        // Verify expected_player_index matches (race condition guard)
        {
            let timers = self.active_timers.lock().unwrap();
            if let Some(timer) = timers.get(&game_id) {
                if timer.expected_player_index != player_idx {
                    return;
                }
            }
        }

        // Set clock to 0 for the timed-out player
        {
            let games = match self.games.read() {
                Ok(g) => g,
                Err(_) => return,
            };
            if let Some(game_lock) = games.get(&game_id) {
                if let Ok(mut game) = game_lock.write() {
                    if let Some(clocks) = game.get_player_clocks_mut() {
                        clocks.remaining_ms[player_idx] = 0;
                    }
                }
            }
        }

        // Clean up the timer entry (timeout already fired, just remove state)
        {
            let mut timers = self.active_timers.lock().unwrap();
            timers.remove(&game_id);
        }

        if is_first_round_betting {
            // ABORT: Game is cancelled
            self.abort_game(game_id, "Player timed out during first round betting".to_string());
        } else {
            match current_state {
                State::Betting(_) => {
                    // Auto-bet 1
                    let _ = self.make_transition_internal(game_id, GameTransition::Bet(1), true);
                }
                State::Trick(_) => {
                    // Auto-play a random legal card
                    let card = {
                        let games = match self.games.read() {
                            Ok(g) => g,
                            Err(_) => return,
                        };
                        let game_lock = match games.get(&game_id) {
                            Some(g) => g,
                            None => return,
                        };
                        let game = match game_lock.read() {
                            Ok(g) => g,
                            Err(_) => return,
                        };
                        match game.get_legal_cards() {
                            Ok(cards) if !cards.is_empty() => {
                                let mut rng = rand::thread_rng();
                                cards.choose(&mut rng).cloned()
                            }
                            _ => None,
                        }
                    };

                    if let Some(card) = card {
                        let _ = self.make_transition_internal(game_id, GameTransition::Card(card), true);
                    }
                }
                _ => {}
            }
        }
    }

    /// Abort a game (used for round 1 betting timeout)
    fn abort_game(&self, game_id: Uuid, reason: String) {
        {
            let games = match self.games.read() {
                Ok(g) => g,
                Err(_) => return,
            };
            if let Some(game_lock) = games.get(&game_id) {
                if let Ok(mut game) = game_lock.write() {
                    game.set_state(State::Aborted);
                    game.set_turn_started_at_epoch_ms(None);
                    self.persist_update(&game);
                }
            }
        }

        if let Ok(broadcasters) = self.broadcasters.read() {
            if let Some(tx) = broadcasters.get(&game_id) {
                let _ = tx.send(GameEvent::GameAborted {
                    game_id,
                    reason,
                });
            }
        }
    }

    /// Set a player's display name
    pub fn set_player_name(&self, game_id: Uuid, player_id: Uuid, name: Option<String>)
        -> Result<(), GameManagerError> {
        let games = self.games.read().map_err(|_| GameManagerError::LockError)?;
        let game_lock = games.get(&game_id).ok_or(GameManagerError::GameNotFound)?;
        let mut game = game_lock.write().map_err(|_| GameManagerError::LockError)?;

        game.set_player_name(player_id, name)
            .map_err(|e| GameManagerError::GameError(format!("{:?}", e)))?;

        self.persist_update(&game);

        let state_response = Self::build_state_response(game_id, &game, None);

        drop(game);
        drop(games);

        if let Ok(broadcasters) = self.broadcasters.read() {
            if let Some(tx) = broadcasters.get(&game_id) {
                let _ = tx.send(GameEvent::StateChanged(state_response));
            }
        }

        Ok(())
    }

    /// List all active games
    pub fn list_games(&self) -> Result<Vec<Uuid>, GameManagerError> {
        let games = self.games.read().map_err(|_| GameManagerError::LockError)?;
        Ok(games.keys().copied().collect())
    }

    /// Remove a completed game
    pub fn remove_game(&self, game_id: Uuid) -> Result<(), GameManagerError> {
        // Cancel any active timer
        {
            let mut timers = self.active_timers.lock().unwrap();
            if let Some(timer) = timers.remove(&game_id) {
                timer.timeout_handle.abort();
            }
        }

        let mut games = self.games.write().map_err(|_| GameManagerError::LockError)?;
        games.remove(&game_id).ok_or(GameManagerError::GameNotFound)?;
        drop(games);

        self.persist_delete(game_id);

        if let Ok(mut broadcasters) = self.broadcasters.write() {
            broadcasters.remove(&game_id);
        }

        Ok(())
    }

    /// Subscribe to game state change events
    pub fn subscribe(&self, game_id: Uuid) -> Result<broadcast::Receiver<GameEvent>, GameManagerError> {
        let broadcasters = self.broadcasters.read().map_err(|_| GameManagerError::LockError)?;
        let tx = broadcasters.get(&game_id).ok_or(GameManagerError::GameNotFound)?;
        Ok(tx.subscribe())
    }
}

impl Default for GameManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_game() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();

        assert_ne!(response.game_id, Uuid::nil());
        assert_eq!(response.player_ids.len(), 4);
    }

    #[test]
    fn test_get_game_state() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();

        let state = manager.get_game_state(response.game_id).unwrap();
        assert_eq!(state.state, State::NotStarted);
        assert_eq!(state.game_id, response.game_id);
    }

    #[test]
    fn test_make_transition() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();

        let result = manager.make_transition(response.game_id, GameTransition::Start).unwrap();
        assert_eq!(result, TransitionSuccess::Start);

        let state = manager.get_game_state(response.game_id).unwrap();
        assert_eq!(state.state, State::Betting(0));
    }

    #[test]
    fn test_list_games() {
        let manager = GameManager::new();
        let game1 = manager.create_game(500, None).unwrap();
        let game2 = manager.create_game(500, None).unwrap();

        let games = manager.list_games().unwrap();
        assert_eq!(games.len(), 2);
        assert!(games.contains(&game1.game_id));
        assert!(games.contains(&game2.game_id));
    }

    #[test]
    fn test_remove_game() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();

        manager.remove_game(response.game_id).unwrap();

        let games = manager.list_games().unwrap();
        assert_eq!(games.len(), 0);
    }

    #[test]
    fn test_create_game_with_players() {
        let manager = GameManager::new();
        let player_ids = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];
        let response = manager.create_game_with_players(player_ids, 500, None).unwrap();

        assert_eq!(response.player_ids, player_ids);
        assert_ne!(response.game_id, Uuid::nil());

        let state = manager.get_game_state(response.game_id).unwrap();
        assert_eq!(state.state, State::NotStarted);
    }

    #[test]
    fn test_create_timed_game() {
        let manager = GameManager::new();
        let tc = TimerConfig { initial_time_secs: 300, increment_secs: 5 };
        let response = manager.create_game(500, Some(tc)).unwrap();

        let state = manager.get_game_state(response.game_id).unwrap();
        assert_eq!(state.timer_config, Some(tc));
        assert_eq!(state.player_clocks_ms, Some([300_000; 4]));
    }

    #[test]
    fn test_untimed_game_no_timer_data() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();

        let state = manager.get_game_state(response.game_id).unwrap();
        assert!(state.timer_config.is_none());
        assert!(state.player_clocks_ms.is_none());
        assert!(state.active_player_clock_ms.is_none());
    }

    #[test]
    fn test_get_hand_valid_player() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        manager.make_transition(response.game_id, GameTransition::Start).unwrap();

        let hand = manager.get_hand(response.game_id, response.player_ids[0]).unwrap();
        assert_eq!(hand.player_id, response.player_ids[0]);
        assert_eq!(hand.cards.len(), 13);
    }

    #[test]
    fn test_get_hand_invalid_player() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        manager.make_transition(response.game_id, GameTransition::Start).unwrap();

        let result = manager.get_hand(response.game_id, Uuid::new_v4());
        assert!(matches!(result, Err(GameManagerError::GameError(_))));
    }

    #[test]
    fn test_get_hand_game_not_found() {
        let manager = GameManager::new();
        let result = manager.get_hand(Uuid::new_v4(), Uuid::new_v4());
        assert!(matches!(result, Err(GameManagerError::GameNotFound)));
    }

    #[test]
    fn test_make_transition_game_not_found() {
        let manager = GameManager::new();
        let result = manager.make_transition(Uuid::new_v4(), GameTransition::Start);
        assert!(matches!(result, Err(GameManagerError::GameNotFound)));
    }

    #[test]
    fn test_make_transition_start_twice() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        manager.make_transition(response.game_id, GameTransition::Start).unwrap();

        let result = manager.make_transition(response.game_id, GameTransition::Start);
        assert!(matches!(result, Err(GameManagerError::GameError(_))));
    }

    #[test]
    fn test_make_transition_bet() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        manager.make_transition(response.game_id, GameTransition::Start).unwrap();

        let result = manager.make_transition(response.game_id, GameTransition::Bet(3)).unwrap();
        assert_eq!(result, TransitionSuccess::Bet);
    }

    #[test]
    fn test_set_player_name_valid() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();

        manager.set_player_name(response.game_id, response.player_ids[0], Some("Alice".to_string())).unwrap();

        let state = manager.get_game_state(response.game_id).unwrap();
        assert_eq!(state.player_names[0].name.as_deref(), Some("Alice"));
    }

    #[test]
    fn test_set_player_name_invalid_uuid() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();

        let result = manager.set_player_name(response.game_id, Uuid::new_v4(), Some("Nobody".to_string()));
        assert!(matches!(result, Err(GameManagerError::GameError(_))));
    }

    #[test]
    fn test_set_player_name_game_not_found() {
        let manager = GameManager::new();
        let result = manager.set_player_name(Uuid::new_v4(), Uuid::new_v4(), Some("Test".to_string()));
        assert!(matches!(result, Err(GameManagerError::GameNotFound)));
    }

    #[test]
    fn test_subscribe_valid_game() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();

        let rx = manager.subscribe(response.game_id);
        assert!(rx.is_ok());
    }

    #[test]
    fn test_subscribe_game_not_found() {
        let manager = GameManager::new();
        let result = manager.subscribe(Uuid::new_v4());
        assert!(matches!(result, Err(GameManagerError::GameNotFound)));
    }

    #[test]
    fn test_subscribe_receives_state_changed() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        let mut rx = manager.subscribe(response.game_id).unwrap();

        manager.make_transition(response.game_id, GameTransition::Start).unwrap();

        let event = rx.try_recv().unwrap();
        match event {
            GameEvent::StateChanged(state) => {
                assert_eq!(state.state, State::Betting(0));
            }
            _ => panic!("Expected StateChanged event"),
        }
    }

    #[test]
    fn test_remove_game_not_found() {
        let manager = GameManager::new();
        let result = manager.remove_game(Uuid::new_v4());
        assert!(matches!(result, Err(GameManagerError::GameNotFound)));
    }

    #[test]
    fn test_get_game_state_not_found() {
        let manager = GameManager::new();
        let result = manager.get_game_state(Uuid::new_v4());
        assert!(matches!(result, Err(GameManagerError::GameNotFound)));
    }

    #[test]
    fn test_game_event_serde_state_changed() {
        let state = GameStateResponse {
            game_id: Uuid::nil(),
            state: State::NotStarted,
            team_a_score: None,
            team_b_score: None,
            team_a_bags: None,
            team_b_bags: None,
            current_player_id: None,
            player_names: [
                PlayerNameEntry { player_id: Uuid::nil(), name: None },
                PlayerNameEntry { player_id: Uuid::nil(), name: None },
                PlayerNameEntry { player_id: Uuid::nil(), name: None },
                PlayerNameEntry { player_id: Uuid::nil(), name: None },
            ],
            timer_config: None,
            player_clocks_ms: None,
            active_player_clock_ms: None,
        };
        let event = GameEvent::StateChanged(state);
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: GameEvent = serde_json::from_str(&json).unwrap();
        match deserialized {
            GameEvent::StateChanged(s) => assert_eq!(s.state, State::NotStarted),
            _ => panic!("Expected StateChanged"),
        }
    }

    #[test]
    fn test_game_event_serde_game_aborted() {
        let event = GameEvent::GameAborted {
            game_id: Uuid::nil(),
            reason: "timeout".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: GameEvent = serde_json::from_str(&json).unwrap();
        match deserialized {
            GameEvent::GameAborted { reason, .. } => assert_eq!(reason, "timeout"),
            _ => panic!("Expected GameAborted"),
        }
    }

    #[test]
    fn test_default_trait() {
        let manager = GameManager::default();
        let games = manager.list_games().unwrap();
        assert_eq!(games.len(), 0);
    }

    #[test]
    fn test_with_db_empty() {
        let manager = GameManager::with_db(":memory:").unwrap();
        let games = manager.list_games().unwrap();
        assert_eq!(games.len(), 0);
    }

    #[test]
    fn test_with_db_persist_and_reload() {
        let dir = std::env::temp_dir().join(format!("spades_test_{}", Uuid::new_v4()));
        let db_path = dir.to_str().unwrap().to_string();

        // Create a manager, add a game, persist it
        {
            let manager = GameManager::with_db(&db_path).unwrap();
            manager.create_game(500, None).unwrap();
            assert_eq!(manager.list_games().unwrap().len(), 1);
        }

        // Re-open the db and verify the game was loaded
        {
            let manager = GameManager::with_db(&db_path).unwrap();
            assert_eq!(manager.list_games().unwrap().len(), 1);
        }

        // Clean up
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn test_full_game_bet_and_play_card() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        let game_id = response.game_id;

        // Start the game
        manager.make_transition(game_id, GameTransition::Start).unwrap();

        // Place 4 bets
        for _ in 0..4 {
            manager.make_transition(game_id, GameTransition::Bet(3)).unwrap();
        }

        let state = manager.get_game_state(game_id).unwrap();
        assert!(matches!(state.state, State::Trick(0)));

        // Play a valid card from the current player's hand
        let current_pid = state.current_player_id.unwrap();
        let hand = manager.get_hand(game_id, current_pid).unwrap();
        let card = hand.cards[0].clone();
        let result = manager.make_transition(game_id, GameTransition::Card(card));
        assert!(result.is_ok());
    }

    #[test]
    fn test_persist_operations_with_db() {
        let manager = GameManager::with_db(":memory:").unwrap();
        let response = manager.create_game(500, None).unwrap();

        // Start game (triggers persist_update)
        manager.make_transition(response.game_id, GameTransition::Start).unwrap();

        // Set player name (triggers persist_update)
        manager.set_player_name(response.game_id, response.player_ids[0], Some("Alice".to_string())).unwrap();

        // Remove game (triggers persist_delete)
        manager.remove_game(response.game_id).unwrap();
        assert!(manager.list_games().unwrap().is_empty());
    }

    #[test]
    fn test_remove_game_cancels_timer() {
        let manager = GameManager::new();
        let tc = TimerConfig { initial_time_secs: 300, increment_secs: 5 };
        let response = manager.create_game(500, Some(tc)).unwrap();
        // Remove immediately — should not panic even though no timer is active
        manager.remove_game(response.game_id).unwrap();
    }

    #[test]
    fn test_build_state_response_with_timer() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        manager.make_transition(response.game_id, GameTransition::Start).unwrap();

        // This exercises build_state_response_with_timer (no active timer)
        let state = manager.build_state_response_with_timer(
            response.game_id,
            &crate::Game::new(response.game_id, response.player_ids, 500, None),
        );
        assert_eq!(state.game_id, response.game_id);
    }

    #[test]
    fn test_transition_request_type_serde() {
        let start = TransitionRequestType::Start;
        let json = serde_json::to_string(&start).unwrap();
        let _: TransitionRequestType = serde_json::from_str(&json).unwrap();

        let bet = TransitionRequestType::Bet { amount: 3 };
        let json = serde_json::to_string(&bet).unwrap();
        let _: TransitionRequestType = serde_json::from_str(&json).unwrap();

        let card = TransitionRequestType::Card {
            card: crate::Card { suit: crate::Suit::Heart, rank: crate::Rank::Ace },
        };
        let json = serde_json::to_string(&card).unwrap();
        let _: TransitionRequestType = serde_json::from_str(&json).unwrap();
    }

    #[tokio::test]
    async fn test_timed_game_start_and_bet() {
        let manager = GameManager::new();
        let tc = TimerConfig { initial_time_secs: 300, increment_secs: 5 };
        let response = manager.create_game(500, Some(tc)).unwrap();
        let game_id = response.game_id;

        // Start timed game - should start turn timer
        let result = manager.make_transition(game_id, GameTransition::Start).unwrap();
        assert_eq!(result, TransitionSuccess::Start);

        // Check state has timer data
        let state = manager.get_game_state(game_id).unwrap();
        assert!(state.timer_config.is_some());
        assert!(state.player_clocks_ms.is_some());

        // Place 4 bets — each cancels old timer and starts new one
        for _ in 0..4 {
            manager.make_transition(game_id, GameTransition::Bet(3)).unwrap();
        }

        let state = manager.get_game_state(game_id).unwrap();
        assert!(matches!(state.state, State::Trick(0)));

        // Play a valid card
        let current_pid = state.current_player_id.unwrap();
        let hand = manager.get_hand(game_id, current_pid).unwrap();
        let card = hand.cards[0].clone();
        manager.make_transition(game_id, GameTransition::Card(card)).unwrap();
    }

    #[tokio::test]
    async fn test_timed_game_remove_cancels_timer() {
        let manager = GameManager::new();
        let tc = TimerConfig { initial_time_secs: 300, increment_secs: 5 };
        let response = manager.create_game(500, Some(tc)).unwrap();

        manager.make_transition(response.game_id, GameTransition::Start).unwrap();
        // Timer is now active — removing game should cancel it
        manager.remove_game(response.game_id).unwrap();
    }

    #[tokio::test]
    async fn test_timed_game_state_has_active_clock() {
        let manager = GameManager::new();
        let tc = TimerConfig { initial_time_secs: 300, increment_secs: 5 };
        let response = manager.create_game(500, Some(tc)).unwrap();

        manager.make_transition(response.game_id, GameTransition::Start).unwrap();

        // Small delay to let timer start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let state = manager.get_game_state(response.game_id).unwrap();
        // active_player_clock_ms should be set since timer is running
        assert!(state.active_player_clock_ms.is_some());
    }

    #[tokio::test]
    async fn test_with_db_persists_timed_game() {
        let dir = std::env::temp_dir().join(format!("spades_timed_{}", Uuid::new_v4()));
        let db_path = dir.to_str().unwrap().to_string();

        // Create timed game, start it, persist
        {
            let manager = GameManager::with_db(&db_path).unwrap();
            let tc = TimerConfig { initial_time_secs: 300, increment_secs: 5 };
            let response = manager.create_game(500, Some(tc)).unwrap();
            manager.make_transition(response.game_id, GameTransition::Start).unwrap();
        }

        // Re-open — should reload and attempt to restart timers
        {
            let manager = GameManager::with_db(&db_path).unwrap();
            assert_eq!(manager.list_games().unwrap().len(), 1);
        }

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn test_game_manager_error_serde() {
        let err = GameManagerError::GameNotFound;
        let json = serde_json::to_string(&err).unwrap();
        let _: GameManagerError = serde_json::from_str(&json).unwrap();

        let err = GameManagerError::LockError;
        let json = serde_json::to_string(&err).unwrap();
        let _: GameManagerError = serde_json::from_str(&json).unwrap();

        let err = GameManagerError::GameError("test".to_string());
        let json = serde_json::to_string(&err).unwrap();
        let _: GameManagerError = serde_json::from_str(&json).unwrap();
    }
}
