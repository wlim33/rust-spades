use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use tokio::sync::broadcast;
use spades::{Game, GameTransition, State, Card, TimerConfig};
use spades::ai::AiStrategy;
use spades::{GetError, TransitionError, TransitionSuccess};
use crate::lock_util::MutexExt;
use crate::sqlite_store::SqliteStore;
use serde::{Serialize, Deserialize};
use rand::seq::SliceRandom;
use tracing::{error, info};

/// Event broadcast to WebSocket subscribers when game state changes.
///
/// `seq` is a per-game monotonically-increasing cursor allocated atomically
/// inside the per-game write lock — events for one game are totally ordered
/// across threads, and a subscriber can detect a missed event by observing
/// a gap in seq.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GameEvent {
    StateChanged {
        seq: u64,
        #[serde(flatten)]
        state: GameStateResponse,
    },
    GameAborted {
        seq: u64,
        game_id: Uuid,
        reason: String,
    },
}

/// Per-game fan-out channel paired with the seq cursor and a ring buffer of
/// recent events for reconnection catch-up. The cursor is the seq value that
/// will be assigned to the NEXT event; the buffer retains the most recent
/// `RECENT_CAP` events so a briefly-disconnected subscriber can replay what
/// they missed instead of pulling a fresh snapshot.
pub struct GameBroadcaster {
    pub sender: broadcast::Sender<GameEvent>,
    pub next_seq: AtomicU64,
    pub recent: Mutex<VecDeque<GameEvent>>,
}

/// Number of most-recent events retained per game for `?since=N` catch-up.
/// Sized at 2× the broadcast::channel buffer so a subscriber that triggers
/// `Lagged` still has hope of catching up via the ring buffer instead of
/// being forced to re-snapshot.
pub const RECENT_CAP: usize = 128;

impl GameBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            next_seq: AtomicU64::new(0),
            recent: Mutex::new(VecDeque::with_capacity(RECENT_CAP)),
        }
    }

    /// Read the current cursor without advancing — used for snapshot stamping.
    pub fn current_seq(&self) -> u64 {
        self.next_seq.load(Ordering::Relaxed)
    }

    /// Allocate a seq, build the event, retain it in the ring buffer, and
    /// publish to subscribers. Returns the seq value assigned.
    pub fn broadcast<F>(&self, build: F) -> u64
    where
        F: FnOnce(u64) -> GameEvent,
    {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        let event = build(seq);
        {
            let mut recent = self.recent.lock_or_recover();
            recent.push_back(event.clone());
            while recent.len() > RECENT_CAP {
                recent.pop_front();
            }
        }
        let _ = self.sender.send(event);
        seq
    }

    /// Return events with seq >= `since` from the ring buffer, in seq order.
    /// `None` means the buffer no longer holds anything that old, or the
    /// caller's cursor is past `current_seq` (suggesting state from a prior
    /// server lifetime) — caller should fall back to a fresh snapshot.
    /// `Some(empty)` means the caller is exactly up to date.
    pub fn catch_up_since(&self, since: u64, current_seq: u64) -> Option<Vec<GameEvent>> {
        if since == current_seq {
            return Some(Vec::new());
        }
        if since > current_seq {
            return None;
        }
        let recent = self.recent.lock_or_recover();
        let front_seq = recent.front().map(event_seq)?;
        if front_seq > since {
            return None;
        }
        Some(
            recent
                .iter()
                .filter(|e| {
                    let seq = event_seq(e);
                    seq >= since && seq < current_seq
                })
                .cloned()
                .collect(),
        )
    }
}

/// Extract the seq from a `GameEvent` regardless of variant.
pub fn event_seq(event: &GameEvent) -> u64 {
    match event {
        GameEvent::StateChanged { seq, .. } => *seq,
        GameEvent::GameAborted { seq, .. } => *seq,
    }
}

/// Outcome of subscribing to a game's event stream. Captures the receiver,
/// the seq cursor, a snapshot of the current state, and either a fresh
/// snapshot path or a catch-up event list depending on whether the caller
/// passed `?since=N` and whether the ring buffer still holds the requested
/// events.
pub struct Subscription {
    pub rx: broadcast::Receiver<GameEvent>,
    pub current_seq: u64,
    pub initial_state: GameStateResponse,
    /// `Some(events)` when the caller passed `since` and the ring buffer
    /// holds the events from there forward; the WS handler should replay
    /// those instead of sending `initial_state`. `None` means the WS
    /// handler should send `initial_state` as a fresh snapshot — either
    /// no `since` was given, the cursor was beyond `current_seq`, or the
    /// ring buffer was pruned past it.
    pub catch_up: Option<Vec<GameEvent>>,
}

/// Runtime-only timer state for an active turn (not serialized)
struct ActiveTurnTimer {
    turn_started_at: tokio::time::Instant,
    remaining_at_turn_start_ms: u64,
    timeout_handle: tokio::task::JoinHandle<()>,
    expected_player_index: usize,
}

/// Configuration for AI players in a game
pub struct AiPlayerConfig {
    pub ai_players: HashSet<usize>,
    pub strategy: Arc<dyn AiStrategy>,
}

/// Manages multiple concurrent spades games
#[derive(Clone)]
pub struct GameManager {
    games: Arc<RwLock<HashMap<Uuid, Arc<RwLock<Game>>>>>,
    broadcasters: Arc<RwLock<HashMap<Uuid, Arc<GameBroadcaster>>>>,
    db: Option<Arc<SqliteStore>>,
    active_timers: Arc<Mutex<HashMap<Uuid, ActiveTurnTimer>>>,
    ai_configs: Arc<RwLock<HashMap<Uuid, AiPlayerConfig>>>,
}

/// Response for creating a new game
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateGameResponse {
    pub game_id: Uuid,
    pub player_ids: [Uuid; 4],
}

/// A player's name entry
#[derive(Debug, Clone, Serialize, Deserialize, oasgen::OaSchema)]
pub struct PlayerNameEntry {
    pub player_id: Uuid,
    pub name: Option<String>,
}

/// Response for getting game state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStateResponse {
    pub game_id: Uuid,
    pub short_id: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_cards: Option<[Option<Card>; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_bets: Option<[i32; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_tricks_won: Option<[i32; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_trick_winner_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_completed_trick: Option<[Card; 4]>,
}

/// Response for getting a player's hand
#[derive(Debug, Serialize, Deserialize, oasgen::OaSchema)]
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
    Transition(TransitionError),
    Get(GetError),
    LockError,
}

impl std::fmt::Display for GameManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GameNotFound => write!(f, "Game not found"),
            Self::Transition(e) => write!(f, "{e}"),
            Self::Get(e) => write!(f, "{e}"),
            Self::LockError => write!(f, "Internal lock error"),
        }
    }
}

impl std::error::Error for GameManagerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transition(e) => Some(e),
            Self::Get(e) => Some(e),
            _ => None,
        }
    }
}

impl From<TransitionError> for GameManagerError {
    fn from(e: TransitionError) -> Self { Self::Transition(e) }
}

impl From<GetError> for GameManagerError {
    fn from(e: GetError) -> Self { Self::Get(e) }
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
            ai_configs: Arc::new(RwLock::new(HashMap::new())),
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
            broadcasters_map.insert(id, Arc::new(GameBroadcaster::new(64)));
        }
        let count = games_map.len();
        info!(count, "loaded games from database");
        let manager = GameManager {
            games: Arc::new(RwLock::new(games_map)),
            broadcasters: Arc::new(RwLock::new(broadcasters_map)),
            db: Some(Arc::new(store)),
            active_timers: Arc::new(Mutex::new(HashMap::new())),
            ai_configs: Arc::new(RwLock::new(HashMap::new())),
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
        if let Some(db) = &self.db
            && let Err(e) = db.insert_game(game) {
                error!(game_id = %game.get_id(), error = %e, "failed to persist game insert");
            }
    }

    fn persist_update(&self, game: &Game) {
        if let Some(db) = &self.db
            && let Err(e) = db.update_game(game) {
                error!(game_id = %game.get_id(), error = %e, "failed to persist game update");
            }
    }

    fn persist_delete(&self, game_id: Uuid) {
        if let Some(db) = &self.db
            && let Err(e) = db.delete_game(game_id) {
                error!(game_id = %game_id, error = %e, "failed to persist game delete");
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

        let mut broadcasters = self.broadcasters.write().map_err(|_| GameManagerError::LockError)?;
        broadcasters.insert(game_id, Arc::new(GameBroadcaster::new(64)));

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

        let mut broadcasters = self.broadcasters.write().map_err(|_| GameManagerError::LockError)?;
        broadcasters.insert(game_id, Arc::new(GameBroadcaster::new(64)));

        Ok(CreateGameResponse {
            game_id,
            player_ids,
        })
    }

    /// Create a game with AI players filling non-human seats.
    /// `human_seats` specifies which seat indices (0-3) are human.
    pub fn create_ai_game(
        &self,
        human_seats: HashSet<usize>,
        max_points: i32,
        timer_config: Option<TimerConfig>,
        strategy: Arc<dyn AiStrategy>,
    ) -> Result<CreateGameResponse, GameManagerError> {
        let response = self.create_game(max_points, timer_config)?;
        let game_id = response.game_id;

        let ai_players: HashSet<usize> = (0..4)
            .filter(|i| !human_seats.contains(i))
            .collect();

        let config = AiPlayerConfig {
            ai_players,
            strategy,
        };
        let mut configs = self.ai_configs.write().map_err(|_| GameManagerError::LockError)?;
        configs.insert(game_id, config);

        Ok(response)
    }

    /// After a transition, auto-play consecutive AI turns until a human's turn or game end.
    pub fn play_ai_turns(&self, game_id: Uuid) -> Result<(), GameManagerError> {
        loop {
            let (state, player_index) = {
                let games = self.games.read().map_err(|_| GameManagerError::LockError)?;
                let game_lock = games.get(&game_id).ok_or(GameManagerError::GameNotFound)?;
                let game = game_lock.read().map_err(|_| GameManagerError::LockError)?;
                (game.get_state().clone(), game.get_current_player_index_num())
            };

            match state {
                State::Completed | State::Aborted | State::NotStarted => break,
                State::Betting(_) | State::Trick(_) => {}
            }

            let transition = {
                let configs = self.ai_configs.read().map_err(|_| GameManagerError::LockError)?;
                let config = match configs.get(&game_id) {
                    Some(c) => c,
                    None => break, // no AI config = all human
                };
                if !config.ai_players.contains(&player_index) {
                    break; // human's turn
                }

                let games = self.games.read().map_err(|_| GameManagerError::LockError)?;
                let game_lock = games.get(&game_id).ok_or(GameManagerError::GameNotFound)?;
                let game = game_lock.read().map_err(|_| GameManagerError::LockError)?;

                match state {
                    State::Betting(_) => {
                        GameTransition::Bet(config.strategy.choose_bet(&game, player_index))
                    }
                    State::Trick(_) => {
                        GameTransition::Card(config.strategy.choose_card(&game, player_index))
                    }
                    _ => break,
                }
            };

            self.make_transition_internal(game_id, transition, false)?;
        }
        Ok(())
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

        let table_cards = game.get_current_trick_cards().ok().cloned();

        GameStateResponse {
            game_id,
            short_id: spades::uuid_to_short_id(game_id),
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
            table_cards,
            player_bets: game.get_player_bets(),
            player_tricks_won: game.get_player_tricks_won(),
            last_trick_winner_id: game.get_last_trick_winner_id(),
            last_completed_trick: game.get_last_completed_trick().cloned(),
        }
    }

    /// Build state response with real-time timer data
    pub fn build_state_response_with_timer(&self, game_id: Uuid, game: &Game) -> GameStateResponse {
        let timers = self.active_timers.lock_or_recover();
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

        let cards = game.get_hand_by_player_id(player_id)?
            .clone();

        Ok(HandResponse {
            player_id,
            cards,
        })
    }

    /// Cancel the active turn timer for a game.
    /// Returns (elapsed_ms, previous_player_index).
    fn cancel_turn_timer(&self, game_id: Uuid) -> (u64, Option<usize>) {
        let mut timers = self.active_timers.lock_or_recover();
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

        let mut timers = self.active_timers.lock_or_recover();
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
            if let Some(idx) = prev_idx
                && let Some(clocks) = game.get_player_clocks_mut() {
                    clocks.remaining_ms[idx] = clocks.remaining_ms[idx].saturating_sub(elapsed_ms) + increment_ms;
                }
        }

        let result = game.play(transition)?;

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

        let timers = self.active_timers.lock_or_recover();
        let active_timer = timers.get(&game_id);
        let state_response = Self::build_state_response(game_id, &game, active_timer);
        drop(timers);

        // Allocate seq + broadcast while the per-game write lock is still held
        // so concurrent transitions see seq values in transition order.
        if let Ok(broadcasters) = self.broadcasters.read()
            && let Some(b) = broadcasters.get(&game_id) {
                b.broadcast(|seq| GameEvent::StateChanged { seq, state: state_response });
            }

        drop(game);
        drop(games);

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
            let timers = self.active_timers.lock_or_recover();
            if let Some(timer) = timers.get(&game_id)
                && timer.expected_player_index != player_idx {
                    return;
                }
        }

        // Set clock to 0 for the timed-out player
        {
            let games = match self.games.read() {
                Ok(g) => g,
                Err(_) => return,
            };
            if let Some(game_lock) = games.get(&game_id)
                && let Ok(mut game) = game_lock.write()
                    && let Some(clocks) = game.get_player_clocks_mut() {
                        clocks.remaining_ms[player_idx] = 0;
                    }
        }

        // Clean up the timer entry (timeout already fired, just remove state)
        {
            let mut timers = self.active_timers.lock_or_recover();
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
        let games = match self.games.read() {
            Ok(g) => g,
            Err(_) => return,
        };
        if let Some(game_lock) = games.get(&game_id)
            && let Ok(mut game) = game_lock.write() {
                game.set_state(State::Aborted);
                game.set_turn_started_at_epoch_ms(None);
                self.persist_update(&game);

                // Broadcast under the write lock so seq matches state order.
                if let Ok(broadcasters) = self.broadcasters.read()
                    && let Some(b) = broadcasters.get(&game_id) {
                        b.broadcast(|seq| GameEvent::GameAborted { seq, game_id, reason });
                    }
            }
    }

    /// Set a player's display name
    pub fn set_player_name(&self, game_id: Uuid, player_id: Uuid, name: Option<String>)
        -> Result<(), GameManagerError> {
        let games = self.games.read().map_err(|_| GameManagerError::LockError)?;
        let game_lock = games.get(&game_id).ok_or(GameManagerError::GameNotFound)?;
        let mut game = game_lock.write().map_err(|_| GameManagerError::LockError)?;

        game.set_player_name(player_id, name)?;

        self.persist_update(&game);

        let state_response = Self::build_state_response(game_id, &game, None);

        // Broadcast under the write lock so seq matches state order.
        if let Ok(broadcasters) = self.broadcasters.read()
            && let Some(b) = broadcasters.get(&game_id) {
                b.broadcast(|seq| GameEvent::StateChanged { seq, state: state_response });
            }

        drop(game);
        drop(games);

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
            let mut timers = self.active_timers.lock_or_recover();
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

        if let Ok(mut configs) = self.ai_configs.write() {
            configs.remove(&game_id);
        }

        Ok(())
    }

    /// Subscribe to a game's event stream. Holds the game read lock through
    /// receiver subscription, seq capture, and snapshot construction so the
    /// returned `(rx, current_seq, initial_state)` triple is atomic — the
    /// receiver delivers exactly the events with seq >= current_seq, and the
    /// snapshot reflects state after all events with seq < current_seq.
    ///
    /// `since`:
    /// * `None` — `catch_up` is `None`; caller sends `initial_state` as a
    ///   fresh snapshot.
    /// * `Some(n)` — caller is reconnecting from seq `n`. If the ring buffer
    ///   still holds events from `n` forward, `catch_up = Some(events)`; the
    ///   caller replays those instead of sending a snapshot. Otherwise the
    ///   gap is too large and `catch_up = None` forces a fresh snapshot.
    pub fn subscribe(
        &self,
        game_id: Uuid,
        since: Option<u64>,
    ) -> Result<Subscription, GameManagerError> {
        let games = self.games.read().map_err(|_| GameManagerError::LockError)?;
        let game_lock = games.get(&game_id).ok_or(GameManagerError::GameNotFound)?;
        let game = game_lock.read().map_err(|_| GameManagerError::LockError)?;

        let broadcasters = self.broadcasters.read().map_err(|_| GameManagerError::LockError)?;
        let b = broadcasters.get(&game_id).ok_or(GameManagerError::GameNotFound)?;

        let current_seq = b.current_seq();
        let rx = b.sender.subscribe();
        let initial_state = self.build_state_response_with_timer(game_id, &game);

        let catch_up = match since {
            None => None,
            Some(n) => b.catch_up_since(n, current_seq),
        };

        Ok(Subscription { rx, current_seq, initial_state, catch_up })
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
        assert!(matches!(result, Err(GameManagerError::Get(GetError::InvalidUuid))));
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
        assert!(matches!(result, Err(GameManagerError::Transition(TransitionError::AlreadyStarted))));
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
        assert!(matches!(result, Err(GameManagerError::Get(GetError::InvalidUuid))));
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

        let sub = manager.subscribe(response.game_id, None);
        assert!(sub.is_ok());
    }

    #[test]
    fn test_subscribe_game_not_found() {
        let manager = GameManager::new();
        let result = manager.subscribe(Uuid::new_v4(), None);
        assert!(matches!(result, Err(GameManagerError::GameNotFound)));
    }

    #[test]
    fn test_subscribe_receives_state_changed() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        let mut sub = manager.subscribe(response.game_id, None).unwrap();
        assert_eq!(sub.current_seq, 0, "fresh game starts at seq 0");
        assert!(sub.catch_up.is_none(), "no since → no catch-up");

        manager.make_transition(response.game_id, GameTransition::Start).unwrap();

        let event = sub.rx.try_recv().unwrap();
        match event {
            GameEvent::StateChanged { seq, state } => {
                assert_eq!(seq, 0, "first broadcast event has seq 0");
                assert_eq!(state.state, State::Betting(0));
            }
            _ => panic!("Expected StateChanged event"),
        }
    }

    #[test]
    fn subscribe_seq_advances_with_each_broadcast() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        let mut sub = manager.subscribe(response.game_id, None).unwrap();
        assert_eq!(sub.current_seq, 0);

        // Start transition + a few bets — each is a broadcast event.
        manager.make_transition(response.game_id, GameTransition::Start).unwrap();
        manager.make_transition(response.game_id, GameTransition::Bet(3)).unwrap();
        manager.make_transition(response.game_id, GameTransition::Bet(3)).unwrap();

        let mut seqs = Vec::new();
        while let Ok(event) = sub.rx.try_recv() {
            if let GameEvent::StateChanged { seq, .. } = event {
                seqs.push(seq);
            }
        }
        assert_eq!(seqs, vec![0, 1, 2], "seq is monotonic from 0");

        // Subscribing again returns the cursor at the post-broadcast value.
        let sub2 = manager.subscribe(response.game_id, None).unwrap();
        assert_eq!(sub2.current_seq, 3, "cursor matches count of broadcasts so far");
    }

    #[test]
    fn subscribe_catch_up_none_when_no_since() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        manager.make_transition(response.game_id, GameTransition::Start).unwrap();
        let sub = manager.subscribe(response.game_id, None).unwrap();
        assert!(sub.catch_up.is_none());
        assert_eq!(sub.current_seq, 1);
    }

    #[test]
    fn subscribe_catch_up_empty_when_caller_up_to_date() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        // No broadcasts yet — current_seq is 0.
        let sub = manager.subscribe(response.game_id, Some(0)).unwrap();
        assert_eq!(sub.current_seq, 0);
        assert!(matches!(sub.catch_up, Some(ref v) if v.is_empty()),
                "since == current_seq → empty replay");

        manager.make_transition(response.game_id, GameTransition::Start).unwrap();
        manager.make_transition(response.game_id, GameTransition::Bet(2)).unwrap();
        let sub = manager.subscribe(response.game_id, Some(2)).unwrap();
        assert_eq!(sub.current_seq, 2);
        assert!(matches!(sub.catch_up, Some(ref v) if v.is_empty()),
                "caught up at seq 2");
    }

    #[test]
    fn subscribe_catch_up_replays_recent_events() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        manager.make_transition(response.game_id, GameTransition::Start).unwrap();
        manager.make_transition(response.game_id, GameTransition::Bet(2)).unwrap();
        manager.make_transition(response.game_id, GameTransition::Bet(2)).unwrap();

        // Client missed seq 1 and 2; current is 3.
        let sub = manager.subscribe(response.game_id, Some(1)).unwrap();
        assert_eq!(sub.current_seq, 3);
        let replay = sub.catch_up.expect("buffer holds events from seq 1");
        assert_eq!(replay.len(), 2, "events 1 and 2 replayed");
        assert_eq!(event_seq(&replay[0]), 1);
        assert_eq!(event_seq(&replay[1]), 2);
    }

    #[test]
    fn subscribe_catch_up_returns_none_when_buffer_pruned() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        // Drive more broadcasts than RECENT_CAP so the buffer prunes the
        // oldest events. set_player_name is the cheapest broadcast path.
        for i in 0..(RECENT_CAP + 50) {
            manager
                .set_player_name(response.game_id, response.player_ids[0], Some(format!("p{i}")))
                .unwrap();
        }
        // Asking from seq 0 — buffer's front is well past that.
        let sub = manager.subscribe(response.game_id, Some(0)).unwrap();
        assert!(sub.catch_up.is_none(), "buffer pruned past seq 0 → snapshot");
    }

    #[test]
    fn subscribe_catch_up_returns_none_when_since_from_future() {
        let manager = GameManager::new();
        let response = manager.create_game(500, None).unwrap();
        manager.make_transition(response.game_id, GameTransition::Start).unwrap();
        // current_seq is 1; client claims they have up to seq 99.
        let sub = manager.subscribe(response.game_id, Some(100)).unwrap();
        assert!(sub.catch_up.is_none(), "since > current_seq → re-snapshot");
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
            short_id: spades::uuid_to_short_id(Uuid::nil()),
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
            table_cards: None,
            player_bets: None,
            player_tricks_won: None,
            last_trick_winner_id: None,
            last_completed_trick: None,
        };
        let event = GameEvent::StateChanged { seq: 7, state };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: GameEvent = serde_json::from_str(&json).unwrap();
        match deserialized {
            GameEvent::StateChanged { seq, state: s } => {
                assert_eq!(seq, 7);
                assert_eq!(s.state, State::NotStarted);
            }
            _ => panic!("Expected StateChanged"),
        }
    }

    #[test]
    fn test_game_event_serde_game_aborted() {
        let event = GameEvent::GameAborted {
            seq: 12,
            game_id: Uuid::nil(),
            reason: "timeout".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: GameEvent = serde_json::from_str(&json).unwrap();
        match deserialized {
            GameEvent::GameAborted { seq, reason, .. } => {
                assert_eq!(seq, 12);
                assert_eq!(reason, "timeout");
            },
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
        let card = hand.cards[0];
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
            &spades::Game::new(response.game_id, response.player_ids, 500, None),
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
            card: spades::Card { suit: spades::Suit::Heart, rank: spades::Rank::Ace },
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
        let card = hand.cards[0];
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

        let err = GameManagerError::Transition(TransitionError::AlreadyStarted);
        let json = serde_json::to_string(&err).unwrap();
        let _: GameManagerError = serde_json::from_str(&json).unwrap();

        let err = GameManagerError::Get(GetError::InvalidUuid);
        let json = serde_json::to_string(&err).unwrap();
        let _: GameManagerError = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_create_ai_game() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let gm = GameManager::new();
            let strategy = Arc::new(spades::ai::RandomStrategy);
            let human_seats: HashSet<usize> = [0].into_iter().collect();
            let response = gm.create_ai_game(human_seats, 500, None, strategy).unwrap();
            assert_ne!(response.game_id, Uuid::nil());
        });
    }

    #[test]
    fn test_ai_auto_plays_betting() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let gm = GameManager::new();
            let strategy = Arc::new(spades::ai::RandomStrategy);
            let human_seats: HashSet<usize> = [0].into_iter().collect();
            let response = gm.create_ai_game(human_seats, 500, None, strategy).unwrap();
            let game_id = response.game_id;

            // Start the game
            gm.make_transition(game_id, GameTransition::Start).unwrap();
            // Player 0 (human) bets
            gm.make_transition(game_id, GameTransition::Bet(3)).unwrap();
            // Auto-play AI turns (players 1, 2, 3)
            gm.play_ai_turns(game_id).unwrap();

            // Should now be in Trick state (all bets placed) with player 0's turn
            let state = gm.get_game_state(game_id).unwrap();
            assert!(matches!(state.state, State::Trick(0)));
            assert_eq!(state.current_player_id, Some(response.player_ids[0]));
        });
    }

    #[test]
    fn test_ai_full_game_1_human_3_ai() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let gm = GameManager::new();
            let strategy = Arc::new(spades::ai::RandomStrategy);
            let human_seats: HashSet<usize> = [0].into_iter().collect();
            let response = gm.create_ai_game(human_seats, 200, None, strategy).unwrap();
            let game_id = response.game_id;
            let human_player_id = response.player_ids[0];

            gm.make_transition(game_id, GameTransition::Start).unwrap();

            loop {
                // Let AI play its turns first
                gm.play_ai_turns(game_id).unwrap();

                let state = gm.get_game_state(game_id).unwrap();
                if state.state == State::Completed {
                    break;
                }

                // Human's turn — make a move
                if state.current_player_id == Some(human_player_id) {
                    if matches!(state.state, State::Betting(_)) {
                        gm.make_transition(game_id, GameTransition::Bet(3)).unwrap();
                    } else {
                        // Get hand and try each card until one succeeds
                        let hand = gm.get_hand(game_id, human_player_id).unwrap();
                        let mut played = false;
                        for card in &hand.cards {
                            if gm.make_transition(game_id, GameTransition::Card(*card)).is_ok() {
                                played = true;
                                break;
                            }
                        }
                        assert!(played, "No legal card found in hand");
                    }
                }
            }

            let final_state = gm.get_game_state(game_id).unwrap();
            assert_eq!(final_state.state, State::Completed);
        });
    }
}
