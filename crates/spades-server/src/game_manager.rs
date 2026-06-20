use crate::game_actor::{GameActor, GameHandle};
use crate::lock_util::{MutexExt, RwLockExt};
use crate::sqlite_store::SqliteStore;
use serde::{Deserialize, Serialize};
use spades::ai::AiStrategy;
use spades::{Card, Game, GameTransition, State, TimerConfig};
use spades::{GetError, TransitionError, TransitionSuccess};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;
use tracing::{error, info};
use uuid::Uuid;

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
    /// Public chat for the table. Spectators receive these too; only seat
    /// owners can send (gated at the HTTP handler).
    ChatMessage {
        seq: u64,
        game_id: Uuid,
        player_id: Uuid,
        content: String,
    },
}

/// Extract the seq from a `GameEvent` regardless of variant.
pub fn event_seq(event: &GameEvent) -> u64 {
    match event {
        GameEvent::StateChanged { seq, .. } => *seq,
        GameEvent::GameAborted { seq, .. } => *seq,
        GameEvent::ChatMessage { seq, .. } => *seq,
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

/// Configuration for AI players in a game. Constructed by `create_ai_game`
/// and handed to the `GameActor` to drive the AI's bets/plays from inside
/// the actor's `ApplyTransition` handler.
pub struct AiPlayerConfig {
    pub ai_players: HashSet<usize>,
    pub strategy: Arc<dyn AiStrategy>,
}

/// Manages the set of running games. Per-game state lives inside the
/// `GameActor` task that each `GameHandle` points to; `GameManager` itself
/// only routes commands by `Uuid`.
///
/// `completed_at` records when each game was first observed in a terminal
/// state (Completed / Aborted). The sweeper uses it to drop the actor
/// from the in-memory map once the configured TTL elapses; subsequent
/// reads of that game rehydrate it from `db` on demand.
#[derive(Clone)]
pub struct GameManager {
    games: Arc<RwLock<HashMap<Uuid, GameHandle>>>,
    db: Option<Arc<SqliteStore>>,
    completed_at: Arc<Mutex<HashMap<Uuid, Instant>>>,
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
    fn from(e: TransitionError) -> Self {
        Self::Transition(e)
    }
}

impl From<GetError> for GameManagerError {
    fn from(e: GetError) -> Self {
        Self::Get(e)
    }
}

pub(crate) fn epoch_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl GameManager {
    /// Create a new game manager (in-memory only).
    pub fn new() -> Self {
        GameManager {
            games: Arc::new(RwLock::new(HashMap::new())),
            db: None,
            completed_at: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a game manager backed by SQLite. Loads existing games from
    /// the database and spawns an actor for each — actors self-restart
    /// any outstanding turn timer on construction.
    pub fn with_db(path: &str) -> Result<Self, String> {
        let store = Arc::new(SqliteStore::open(path)?);
        let existing_games = store.load_all_games()?;
        let count = existing_games.len();
        let mut games_map = HashMap::new();
        for game in existing_games {
            let id = *game.get_id();
            let handle = GameActor::spawn(game, Some(store.clone()), None);
            games_map.insert(id, handle);
        }
        info!(count, "loaded games from database");
        Ok(GameManager {
            games: Arc::new(RwLock::new(games_map)),
            db: Some(store),
            completed_at: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn persist_insert(&self, game: &Game) {
        if let Some(db) = &self.db
            && let Err(e) = db.insert_game(game)
        {
            error!(game_id = %game.get_id(), error = %e, "failed to persist game insert");
        }
    }

    fn persist_delete(&self, game_id: Uuid) {
        if let Some(db) = &self.db
            && let Err(e) = db.delete_game(game_id)
        {
            error!(game_id = %game_id, error = %e, "failed to persist game delete");
        }
    }

    /// Spawn a fresh actor for `game` and insert its handle into the
    /// routing map. Returns the inserted `CreateGameResponse`.
    fn spawn_and_insert(
        &self,
        game: Game,
        ai_config: Option<AiPlayerConfig>,
    ) -> Result<CreateGameResponse, GameManagerError> {
        let game_id = *game.get_id();
        let names = game.get_player_names();
        let player_ids = [names[0].0, names[1].0, names[2].0, names[3].0];
        self.persist_insert(&game);
        let handle = GameActor::spawn(game, self.db.clone(), ai_config);
        let mut games = self.games.write_or_recover();
        games.insert(game_id, handle);
        Ok(CreateGameResponse {
            game_id,
            player_ids,
        })
    }

    pub fn create_game(
        &self,
        max_points: i32,
        timer_config: Option<TimerConfig>,
    ) -> Result<CreateGameResponse, GameManagerError> {
        let game_id = Uuid::new_v4();
        let player_ids = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];
        let game = Game::new(game_id, player_ids, max_points, timer_config);
        self.spawn_and_insert(game, None)
    }

    pub fn create_game_with_players(
        &self,
        player_ids: [Uuid; 4],
        max_points: i32,
        timer_config: Option<TimerConfig>,
    ) -> Result<CreateGameResponse, GameManagerError> {
        let game_id = Uuid::new_v4();
        let game = Game::new(game_id, player_ids, max_points, timer_config);
        self.spawn_and_insert(game, None)
    }

    pub fn create_ai_game(
        &self,
        human_seats: HashSet<usize>,
        max_points: i32,
        timer_config: Option<TimerConfig>,
        strategy: Arc<dyn AiStrategy>,
    ) -> Result<CreateGameResponse, GameManagerError> {
        let game_id = Uuid::new_v4();
        let player_ids = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];
        let game = Game::new(game_id, player_ids, max_points, timer_config);
        let ai_players: HashSet<usize> = (0..4).filter(|i| !human_seats.contains(i)).collect();
        self.spawn_and_insert(
            game,
            Some(AiPlayerConfig {
                ai_players,
                strategy,
            }),
        )
    }

    /// Look up the handle for `game_id`. Fast path: in-memory HashMap.
    /// Cold path: load the row from SQLite (on a `spawn_blocking` worker
    /// so we don't stall the runtime), spawn a fresh actor for it, and
    /// insert into the map. The race window between map miss and actor
    /// insertion is handled by re-checking under the write lock.
    async fn handle(&self, game_id: Uuid) -> Result<GameHandle, GameManagerError> {
        {
            let games = self.games.read_or_recover();
            if let Some(h) = games.get(&game_id) {
                return Ok(h.clone());
            }
        }
        let Some(db) = self.db.clone() else {
            return Err(GameManagerError::GameNotFound);
        };
        let loaded = tokio::task::spawn_blocking(move || db.load_game_by_id(game_id))
            .await
            .map_err(|_| GameManagerError::GameNotFound)?
            .map_err(|_| GameManagerError::GameNotFound)?;
        let Some(game) = loaded else {
            return Err(GameManagerError::GameNotFound);
        };
        let new_handle = GameActor::spawn(game, self.db.clone(), None);
        let mut games = self.games.write_or_recover();
        // Concurrent rehydration could have inserted while we were loading;
        // prefer the existing handle and let our just-spawned one drop.
        if let Some(existing) = games.get(&game_id) {
            return Ok(existing.clone());
        }
        games.insert(game_id, new_handle.clone());
        Ok(new_handle)
    }

    pub async fn get_game_state(
        &self,
        game_id: Uuid,
    ) -> Result<GameStateResponse, GameManagerError> {
        self.handle(game_id).await?.get_state().await
    }

    pub async fn get_hand(
        &self,
        game_id: Uuid,
        player_id: Uuid,
    ) -> Result<HandResponse, GameManagerError> {
        self.handle(game_id).await?.get_hand(player_id).await
    }

    pub async fn make_transition(
        &self,
        game_id: Uuid,
        transition: GameTransition,
    ) -> Result<TransitionSuccess, GameManagerError> {
        self.handle(game_id)
            .await?
            .apply_transition(transition)
            .await
    }

    pub async fn set_player_name(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        name: Option<String>,
    ) -> Result<(), GameManagerError> {
        self.handle(game_id)
            .await?
            .set_player_name(player_id, name)
            .await
    }

    pub async fn subscribe(
        &self,
        game_id: Uuid,
        since: Option<u64>,
    ) -> Result<Subscription, GameManagerError> {
        self.handle(game_id).await?.subscribe(since).await
    }

    /// `Ok(Some(text))` for a terminated game; `Ok(None)` for in-progress
    /// (caller should 403 — exposing transcripts mid-game would leak
    /// other players' hidden hands). `Err(GameNotFound)` when neither the
    /// in-memory map nor the DB knows about this id.
    pub async fn get_transcript(&self, game_id: Uuid) -> Result<Option<String>, GameManagerError> {
        self.handle(game_id).await?.get_transcript().await
    }

    pub async fn send_chat(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        content: String,
    ) -> Result<(), GameManagerError> {
        self.handle(game_id)
            .await?
            .send_chat(player_id, content)
            .await
    }

    /// Sweep games that have been in a terminal state (Completed /
    /// Aborted) for at least `ttl`, evicting their actor from the in-memory
    /// map. The next access through `handle()` re-spawns an actor by
    /// loading the row from `db`.
    ///
    /// The first sweep that finds a game in a terminal state records
    /// "completed-at = now" — actual eviction waits `ttl` from that mark.
    /// If a game goes back to non-terminal (impossible in practice, but
    /// defended for robustness), the timestamp is dropped.
    ///
    /// `ttl` is parametric for testability; production uses ~1h.
    pub async fn sweep_completed_games(&self, ttl: Duration) -> usize {
        let now = Instant::now();
        let game_ids: Vec<Uuid> = {
            let games = self.games.read_or_recover();
            games.keys().copied().collect()
        };
        let mut to_evict = Vec::new();
        for game_id in game_ids {
            let handle = {
                let games = self.games.read_or_recover();
                match games.get(&game_id) {
                    Some(h) => h.clone(),
                    None => continue,
                }
            };
            let Ok(state) = handle.get_state().await else {
                continue;
            };
            let is_terminal = matches!(state.state, State::Completed | State::Aborted);
            let should_evict = {
                let mut completed_at = self.completed_at.lock_or_recover();
                if is_terminal {
                    let first_seen = completed_at.entry(game_id).or_insert(now);
                    now.duration_since(*first_seen) >= ttl
                } else {
                    completed_at.remove(&game_id);
                    false
                }
            };
            if should_evict {
                to_evict.push(game_id);
            }
        }
        if to_evict.is_empty() {
            return 0;
        }
        let mut games = self.games.write_or_recover();
        let mut completed_at = self.completed_at.lock_or_recover();
        let mut count = 0;
        for game_id in to_evict {
            if games.remove(&game_id).is_some() {
                completed_at.remove(&game_id);
                // Drop the persisted row too: a terminal game is disposable,
                // so leaving it in the DB only grows the table and the
                // restart-time load/spawn cost (the runaway-game OOM).
                self.persist_delete(game_id);
                count += 1;
            }
        }
        count
    }

    pub fn list_games(&self) -> Result<Vec<Uuid>, GameManagerError> {
        let games = self.games.read_or_recover();
        Ok(games.keys().copied().collect())
    }

    /// Remove a game from the map. Dropping the `GameHandle` closes the
    /// last sender to the actor's mpsc inbox, which lets the actor task
    /// exit on its own. WS subscribers see `RecvError::Closed` as the
    /// broadcast `Sender` drops with the actor.
    pub fn remove_game(&self, game_id: Uuid) -> Result<(), GameManagerError> {
        let mut games = self.games.write_or_recover();
        games
            .remove(&game_id)
            .ok_or(GameManagerError::GameNotFound)?;
        drop(games);
        self.persist_delete(game_id);
        Ok(())
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

    #[tokio::test]
    async fn create_then_state_and_remove() {
        let m = GameManager::new();
        let r = m.create_game(500, None).unwrap();
        let state = m.get_game_state(r.game_id).await.unwrap();
        assert_eq!(state.state, State::NotStarted);
        assert_eq!(state.player_names.len(), 4);
        m.remove_game(r.game_id).unwrap();
        assert!(m.get_game_state(r.game_id).await.is_err());
    }

    #[tokio::test]
    async fn transition_advances_state() {
        let m = GameManager::new();
        let r = m.create_game(500, None).unwrap();
        let out = m
            .make_transition(r.game_id, GameTransition::Start)
            .await
            .unwrap();
        assert_eq!(out, TransitionSuccess::Start);
        let state = m.get_game_state(r.game_id).await.unwrap();
        assert_eq!(state.state, State::Betting(0));
    }

    #[tokio::test]
    async fn missing_game_returns_game_not_found() {
        let m = GameManager::new();
        let id = Uuid::new_v4();
        assert!(matches!(
            m.get_game_state(id).await,
            Err(GameManagerError::GameNotFound)
        ));
        assert!(matches!(
            m.get_hand(id, Uuid::new_v4()).await,
            Err(GameManagerError::GameNotFound)
        ));
        assert!(matches!(
            m.make_transition(id, GameTransition::Start).await,
            Err(GameManagerError::GameNotFound)
        ));
        assert!(matches!(
            m.set_player_name(id, Uuid::new_v4(), None).await,
            Err(GameManagerError::GameNotFound)
        ));
        assert!(matches!(
            m.subscribe(id, None).await,
            Err(GameManagerError::GameNotFound)
        ));
    }

    #[tokio::test]
    async fn subscribe_returns_state_and_seq() {
        let m = GameManager::new();
        let r = m.create_game(500, None).unwrap();
        let sub = m.subscribe(r.game_id, None).await.unwrap();
        assert_eq!(sub.current_seq, 0);
        assert!(sub.catch_up.is_none());
        assert_eq!(sub.initial_state.state, State::NotStarted);
    }

    #[tokio::test]
    async fn subscribe_after_transitions_receives_seq_n() {
        let m = GameManager::new();
        let r = m.create_game(500, None).unwrap();
        m.make_transition(r.game_id, GameTransition::Start)
            .await
            .unwrap();
        // One broadcast happened. New subscriber's cursor should be 1.
        let sub = m.subscribe(r.game_id, None).await.unwrap();
        assert_eq!(sub.current_seq, 1);
    }

    #[tokio::test]
    async fn subscribe_since_returns_empty_when_caller_caught_up() {
        let m = GameManager::new();
        let r = m.create_game(500, None).unwrap();
        m.make_transition(r.game_id, GameTransition::Start)
            .await
            .unwrap();
        let sub = m.subscribe(r.game_id, Some(1)).await.unwrap();
        match sub.catch_up {
            Some(v) => assert!(v.is_empty()),
            None => panic!("expected Some(empty), got None"),
        }
    }

    #[tokio::test]
    async fn list_games_reflects_created_games() {
        let m = GameManager::new();
        let r1 = m.create_game(500, None).unwrap();
        let r2 = m.create_game(500, None).unwrap();
        let mut games = m.list_games().unwrap();
        games.sort();
        let mut expected = vec![r1.game_id, r2.game_id];
        expected.sort();
        assert_eq!(games, expected);
    }

    #[tokio::test]
    async fn set_player_name_persists() {
        let m = GameManager::new();
        let r = m.create_game(500, None).unwrap();
        m.set_player_name(r.game_id, r.player_ids[0], Some("Alice".to_string()))
            .await
            .unwrap();
        let state = m.get_game_state(r.game_id).await.unwrap();
        assert_eq!(state.player_names[0].name.as_deref(), Some("Alice"));
    }

    #[tokio::test]
    async fn get_hand_after_start_returns_13_cards() {
        let m = GameManager::new();
        let r = m.create_game(500, None).unwrap();
        m.make_transition(r.game_id, GameTransition::Start)
            .await
            .unwrap();
        let hand = m.get_hand(r.game_id, r.player_ids[0]).await.unwrap();
        assert_eq!(hand.cards.len(), 13);
        assert_eq!(hand.player_id, r.player_ids[0]);
    }

    #[tokio::test]
    async fn sweep_keeps_active_games() {
        let m = GameManager::new();
        let r = m.create_game(500, None).unwrap();
        let evicted = m.sweep_completed_games(Duration::from_secs(0)).await;
        assert_eq!(
            evicted, 0,
            "active game must not be evicted regardless of ttl"
        );
        assert!(m.list_games().unwrap().contains(&r.game_id));
    }

    #[tokio::test]
    async fn sweep_does_not_evict_until_ttl_elapses() {
        // A zero-second timer means the first player has no time to bet —
        // the actor's first-round-betting timeout fires and aborts the
        // game. Once the abort persists, the game is in a terminal state
        // and the sweeper will mark it; we then assert that a non-zero
        // TTL prevents immediate eviction (it just records first-seen).
        let m = GameManager::new();
        let tc = TimerConfig {
            initial_time_secs: 0,
            increment_secs: 0,
        };
        let r = m.create_game(100, Some(tc)).unwrap();
        m.make_transition(r.game_id, GameTransition::Start)
            .await
            .unwrap();
        // Give the actor's timeout task a moment to fire and abort.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let state = m.get_game_state(r.game_id).await.unwrap();
        assert_eq!(
            state.state,
            State::Aborted,
            "0-second clock should have aborted the game"
        );
        // First sweep with a long TTL: marks the timestamp but doesn't evict.
        let evicted = m.sweep_completed_games(Duration::from_secs(3600)).await;
        assert_eq!(evicted, 0, "first sweep records the mark but waits for ttl");
        assert!(m.list_games().unwrap().contains(&r.game_id));
    }

    #[tokio::test]
    async fn sweep_evicts_after_ttl_elapses() {
        let m = GameManager::new();
        let tc = TimerConfig {
            initial_time_secs: 0,
            increment_secs: 0,
        };
        let r = m.create_game(100, Some(tc)).unwrap();
        m.make_transition(r.game_id, GameTransition::Start)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        // Game has aborted by now. Sweep with ttl=0 evicts immediately
        // (first observation sets timestamp to now, ttl=0 says now-now >= 0).
        let evicted = m.sweep_completed_games(Duration::from_secs(0)).await;
        assert_eq!(
            evicted, 1,
            "ttl=0 should evict the aborted game on first sweep"
        );
        assert!(!m.list_games().unwrap().contains(&r.game_id));
    }

    #[tokio::test]
    async fn sweep_deletes_evicted_game_from_db() {
        // Evicting a terminal game from memory must also delete its
        // persisted row. Otherwise the `games` table grows unbounded and
        // every restart re-loads + re-spawns an actor for each dead game
        // — the runaway-game OOM. The in-memory sweep is the single point
        // that decides a completed game is disposable, so the DB delete
        // rides along with it.
        let store = Arc::new(SqliteStore::open(":memory:").unwrap());
        let m = GameManager {
            games: Arc::new(RwLock::new(HashMap::new())),
            db: Some(store.clone()),
            completed_at: Arc::new(Mutex::new(HashMap::new())),
        };
        let tc = TimerConfig {
            initial_time_secs: 0,
            increment_secs: 0,
        };
        let r = m.create_game(100, Some(tc)).unwrap();
        m.make_transition(r.game_id, GameTransition::Start)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            store.load_all_games().unwrap().len(),
            1,
            "game is persisted before the sweep"
        );
        let evicted = m.sweep_completed_games(Duration::from_secs(0)).await;
        assert_eq!(evicted, 1, "ttl=0 evicts the aborted game");
        assert!(
            store.load_all_games().unwrap().is_empty(),
            "evicted terminal game must also be deleted from the DB"
        );
    }

    #[tokio::test]
    async fn handle_rehydrates_from_db_on_cache_miss() {
        // Insert a game directly into SQLite, then build a manager with
        // that store and an empty in-memory map. The first access has to
        // go through the cold path: load from DB + spawn a fresh actor.
        let store = Arc::new(SqliteStore::open(":memory:").unwrap());
        let game_id = Uuid::new_v4();
        let player_ids = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];
        let game = Game::new(game_id, player_ids, 500, None);
        store.insert_game(&game).unwrap();

        let m = GameManager {
            games: Arc::new(RwLock::new(HashMap::new())),
            db: Some(store),
            completed_at: Arc::new(Mutex::new(HashMap::new())),
        };

        assert!(m.list_games().unwrap().is_empty(), "starts cold");
        let state = m.get_game_state(game_id).await.expect("rehydrated from DB");
        assert_eq!(state.game_id, game_id);
        assert_eq!(state.state, State::NotStarted);
        // Subsequent reads hit the in-memory actor.
        assert_eq!(m.list_games().unwrap(), vec![game_id]);
    }

    #[tokio::test]
    async fn send_chat_broadcasts_to_subscribers() {
        let m = GameManager::new();
        let r = m.create_game(500, None).unwrap();
        let mut sub = m.subscribe(r.game_id, None).await.unwrap();
        m.send_chat(r.game_id, r.player_ids[0], "hello".to_string())
            .await
            .unwrap();
        let event = sub.rx.try_recv().expect("chat event reached subscriber");
        match event {
            GameEvent::ChatMessage {
                game_id,
                player_id,
                content,
                seq,
            } => {
                assert_eq!(game_id, r.game_id);
                assert_eq!(player_id, r.player_ids[0]);
                assert_eq!(content, "hello");
                assert_eq!(seq, 0, "first event on a fresh game is seq 0");
            }
            other => panic!("expected ChatMessage, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn handle_returns_not_found_without_db() {
        let m = GameManager::new();
        let id = Uuid::new_v4();
        assert!(matches!(
            m.get_game_state(id).await,
            Err(GameManagerError::GameNotFound)
        ));
    }
}
