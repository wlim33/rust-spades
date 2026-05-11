//! Per-game actor — one tokio task owns each `Game` and processes commands
//! from an mpsc inbox.
//!
//! The previous design held game state behind three separate maps
//! (`Arc<RwLock<HashMap<Uuid, Arc<RwLock<Game>>>>>` for the game,
//! `Arc<RwLock<HashMap<Uuid, Arc<GameBroadcaster>>>>` for fan-out, and
//! `Arc<Mutex<HashMap<Uuid, ActiveTurnTimer>>>` for timeout tasks) and the
//! transition + timer + broadcast paths each acquired several of those
//! locks in sequence. The serialisation across locks was loose enough that
//! a timer-task firing during another thread's transition could see a
//! stale game state — the explicit race guard at the old
//! `handle_timeout` (matching `expected_player_index` against the current
//! turn) existed precisely because of this. With the actor model the
//! `Game`, the broadcaster, the ring buffer, the active timer, and the AI
//! config all live inside one task's `&mut self`; commands are processed
//! one at a time in mailbox order, and the race guard becomes a single
//! `generation` check on `TimerFired`.

use std::collections::VecDeque;
use std::sync::Arc;

use rand::seq::SliceRandom;
use spades::{Game, GameTransition, State, TransitionSuccess};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::error;
use uuid::Uuid;

use crate::game_manager::{
    AiPlayerConfig, GameEvent, GameManagerError, GameStateResponse, HandResponse,
    PlayerNameEntry, Subscription, epoch_ms_now, event_seq,
};
use crate::ratings::{self, Rating, DEFAULT_RATING};
use crate::sqlite_store::SqliteStore;

/// Per-game broadcast buffer capacity. The corresponding `Lagged` is
/// translated to a `Resync` close frame by the WS handler.
const BROADCAST_CAP: usize = 64;

/// Per-game in-memory event ring buffer capacity. `2× BROADCAST_CAP` so a
/// subscriber that triggers `Lagged` can still potentially catch up via
/// `?since=N` rather than being forced to re-snapshot.
const RECENT_CAP: usize = 128;

/// Commands the actor accepts from its inbox. Every variant that returns a
/// value carries a `oneshot::Sender` for the reply.
pub enum GameCmd {
    ApplyTransition {
        transition: GameTransition,
        reply: oneshot::Sender<Result<TransitionSuccess, GameManagerError>>,
    },
    GetState {
        reply: oneshot::Sender<GameStateResponse>,
    },
    GetHand {
        player_id: Uuid,
        reply: oneshot::Sender<Result<HandResponse, GameManagerError>>,
    },
    SetPlayerName {
        player_id: Uuid,
        name: Option<String>,
        reply: oneshot::Sender<Result<(), GameManagerError>>,
    },
    Subscribe {
        since: Option<u64>,
        reply: oneshot::Sender<Subscription>,
    },
    /// Encode the current game as a transcript. Reply is `Some(text)` only
    /// when the game has reached a terminal state (`Completed` or
    /// `Aborted`); an in-progress game replies `None` so the caller can
    /// 403 the client rather than leak hidden hands. The terminal check
    /// happens inside the actor against the same `self.game` that gets
    /// encoded, so there's no race between "is it over" and "encode".
    GetTranscript {
        reply: oneshot::Sender<Option<String>>,
    },
    /// Broadcast a chat message to all subscribers. Auth happens at the
    /// HTTP handler — the actor trusts the caller.
    SendChat {
        player_id: Uuid,
        content: String,
        reply: oneshot::Sender<()>,
    },
    /// Fired by the spawned timer task. `generation` lets the actor
    /// discard messages from timers that were already cancelled by a
    /// subsequent transition (the spawn-to-mailbox-send hop is not
    /// synchronous with command processing, so `JoinHandle::abort` may
    /// land *after* the timer's sleep completed).
    TimerFired {
        generation: u64,
    },
}

/// A clonable handle to a `GameActor`. Methods are async — each command
/// is sent through the mailbox and the reply is awaited via a `oneshot`.
#[derive(Clone)]
pub struct GameHandle {
    pub game_id: Uuid,
    sender: mpsc::UnboundedSender<GameCmd>,
}

impl GameHandle {
    pub async fn apply_transition(
        &self,
        transition: GameTransition,
    ) -> Result<TransitionSuccess, GameManagerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(GameCmd::ApplyTransition { transition, reply: tx })
            .map_err(|_| GameManagerError::GameNotFound)?;
        rx.await.map_err(|_| GameManagerError::GameNotFound)?
    }

    pub async fn get_state(&self) -> Result<GameStateResponse, GameManagerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(GameCmd::GetState { reply: tx })
            .map_err(|_| GameManagerError::GameNotFound)?;
        rx.await.map_err(|_| GameManagerError::GameNotFound)
    }

    pub async fn get_hand(&self, player_id: Uuid) -> Result<HandResponse, GameManagerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(GameCmd::GetHand { player_id, reply: tx })
            .map_err(|_| GameManagerError::GameNotFound)?;
        rx.await.map_err(|_| GameManagerError::GameNotFound)?
    }

    pub async fn set_player_name(
        &self,
        player_id: Uuid,
        name: Option<String>,
    ) -> Result<(), GameManagerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(GameCmd::SetPlayerName { player_id, name, reply: tx })
            .map_err(|_| GameManagerError::GameNotFound)?;
        rx.await.map_err(|_| GameManagerError::GameNotFound)?
    }

    pub async fn subscribe(&self, since: Option<u64>) -> Result<Subscription, GameManagerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(GameCmd::Subscribe { since, reply: tx })
            .map_err(|_| GameManagerError::GameNotFound)?;
        rx.await.map_err(|_| GameManagerError::GameNotFound)
    }

    /// Returns `Some(transcript_text)` if the game has terminated, `None`
    /// if it's still in progress.
    pub async fn get_transcript(&self) -> Result<Option<String>, GameManagerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(GameCmd::GetTranscript { reply: tx })
            .map_err(|_| GameManagerError::GameNotFound)?;
        rx.await.map_err(|_| GameManagerError::GameNotFound)
    }

    pub async fn send_chat(
        &self,
        player_id: Uuid,
        content: String,
    ) -> Result<(), GameManagerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(GameCmd::SendChat { player_id, content, reply: tx })
            .map_err(|_| GameManagerError::GameNotFound)?;
        rx.await.map_err(|_| GameManagerError::GameNotFound)
    }
}

/// Live timer task state. Held by the actor (no shared lock — the actor's
/// `&mut self` is the only writer).
struct ActiveTimer {
    handle: JoinHandle<()>,
    generation: u64,
    turn_started_at: tokio::time::Instant,
    remaining_at_turn_start_ms: u64,
    expected_player_index: usize,
}

impl Drop for ActiveTimer {
    fn drop(&mut self) {
        // Best-effort cancel. The timer task may have already sent its
        // `TimerFired` to the mailbox; the actor's generation check will
        // discard such stale messages.
        self.handle.abort();
    }
}

pub struct GameActor {
    game_id: Uuid,
    game: Game,
    next_seq: u64,
    recent: VecDeque<GameEvent>,
    sender: broadcast::Sender<GameEvent>,
    active_timer: Option<ActiveTimer>,
    timer_generation: u64,
    ai_config: Option<AiPlayerConfig>,
    db: Option<Arc<SqliteStore>>,
    self_tx: mpsc::UnboundedSender<GameCmd>,
}

impl GameActor {
    /// Spawn an actor task for `game`. Returns the `GameHandle` for the
    /// caller to send commands. The task runs until every `GameHandle`
    /// clone is dropped (the mpsc closes, `inbox.recv()` returns `None`).
    pub fn spawn(
        game: Game,
        db: Option<Arc<SqliteStore>>,
        ai_config: Option<AiPlayerConfig>,
    ) -> GameHandle {
        let game_id = *game.get_id();
        let (broadcast_tx, _) = broadcast::channel(BROADCAST_CAP);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let actor = GameActor {
            game_id,
            game,
            next_seq: 0,
            recent: VecDeque::with_capacity(RECENT_CAP),
            sender: broadcast_tx,
            active_timer: None,
            timer_generation: 0,
            ai_config,
            db,
            self_tx: cmd_tx.clone(),
        };
        tokio::spawn(actor.run(cmd_rx));
        GameHandle { game_id, sender: cmd_tx }
    }

    async fn run(mut self, mut inbox: mpsc::UnboundedReceiver<GameCmd>) {
        // If we're loaded from disk mid-game with an outstanding turn, the
        // game's `turn_started_at_epoch_ms` reflects how much wall-clock
        // time has already burned through the player's remaining clock.
        // Restart the timer so the player still loses on time.
        self.restart_timer_if_in_progress();

        while let Some(cmd) = inbox.recv().await {
            self.handle_cmd(cmd);
        }
        // All handles dropped → actor exits. The broadcast sender drops
        // with `self`, and outstanding `Receiver`s see `RecvError::Closed`.
    }

    fn handle_cmd(&mut self, cmd: GameCmd) {
        match cmd {
            GameCmd::ApplyTransition { transition, reply } => {
                let res = self.handle_transition_full(transition, false);
                let _ = reply.send(res);
            }
            GameCmd::GetState { reply } => {
                let _ = reply.send(self.build_state_response());
            }
            GameCmd::GetHand { player_id, reply } => {
                let res = self
                    .game
                    .get_hand_by_player_id(player_id)
                    .map(|cards| HandResponse { player_id, cards: cards.clone() })
                    .map_err(GameManagerError::from);
                let _ = reply.send(res);
            }
            GameCmd::SetPlayerName { player_id, name, reply } => {
                let res = self.handle_set_player_name(player_id, name);
                let _ = reply.send(res);
            }
            GameCmd::Subscribe { since, reply } => {
                let _ = reply.send(self.build_subscription(since));
            }
            GameCmd::GetTranscript { reply } => {
                let out = match self.game.get_state() {
                    State::Completed | State::Aborted => {
                        Some(spades::transcript::encode(&self.game))
                    }
                    _ => None,
                };
                let _ = reply.send(out);
            }
            GameCmd::SendChat { player_id, content, reply } => {
                self.broadcast(GameEvent::ChatMessage {
                    seq: 0,
                    game_id: self.game_id,
                    player_id,
                    content,
                });
                let _ = reply.send(());
            }
            GameCmd::TimerFired { generation } => {
                self.handle_timer_fired(generation);
            }
        }
    }

    // -- command handlers -----------------------------------------------

    /// Apply a transition and then auto-play any AI turns that follow,
    /// returning the human's transition result. Cascading AI plays happen
    /// in the same mailbox tick — they're direct method calls on `&mut
    /// self`, not re-enqueued commands (which would deadlock).
    fn handle_transition_full(
        &mut self,
        transition: GameTransition,
        is_timeout: bool,
    ) -> Result<TransitionSuccess, GameManagerError> {
        let result = self.apply_one_transition(transition, is_timeout)?;
        while let Some(ai_trans) = self.next_ai_transition() {
            let _ = self.apply_one_transition(ai_trans, false);
        }
        Ok(result)
    }

    fn apply_one_transition(
        &mut self,
        transition: GameTransition,
        is_timeout: bool,
    ) -> Result<TransitionSuccess, GameManagerError> {
        let was_completed = matches!(self.game.get_state(), State::Completed);
        let is_timed = self.game.get_timer_config().is_some();
        let is_start = matches!(transition, GameTransition::Start);

        // Cancel the existing timer + bill the elapsed time to the
        // previous player's clock before applying the move.
        if is_timed && !is_start {
            let (elapsed_ms, prev_idx) = self.cancel_timer();
            let increment_ms = if !is_timeout {
                self.game.get_timer_config().map(|tc| tc.increment_secs * 1000).unwrap_or(0)
            } else {
                0
            };
            if let Some(idx) = prev_idx
                && let Some(clocks) = self.game.get_player_clocks_mut() {
                    clocks.remaining_ms[idx] =
                        clocks.remaining_ms[idx].saturating_sub(elapsed_ms) + increment_ms;
                }
        }

        let success = self.game.play(transition)?;

        // Record when the next player's turn started so a restart can
        // restore the timer accurately.
        if is_timed {
            match self.game.get_state() {
                State::Betting(_) | State::Trick(_) => {
                    self.game.set_turn_started_at_epoch_ms(Some(epoch_ms_now()));
                }
                _ => {
                    self.game.set_turn_started_at_epoch_ms(None);
                }
            }
        }

        // Persist off the actor task — blocking SQL on a spawn_blocking
        // worker so we don't stall the mailbox while the row updates.
        self.persist_async();

        // Start a timer for the next player.
        if is_timed {
            match self.game.get_state() {
                State::Betting(_) | State::Trick(_) => {
                    let player_idx = self.game.get_current_player_index_num();
                    let remaining = self
                        .game
                        .get_player_clocks()
                        .map(|c| c.remaining_ms[player_idx])
                        .unwrap_or(0);
                    self.start_timer(player_idx, remaining);
                }
                _ => {}
            }
        }

        let state_response = self.build_state_response();
        self.broadcast(GameEvent::StateChanged { seq: 0, state: state_response });

        // If this transition was the one that completed the game, fire
        // the Glicko-2 rating update in the background. Aborted games
        // don't trigger updates (handle_timer_fired sets the state
        // directly without going through apply_one_transition).
        if !was_completed && matches!(self.game.get_state(), State::Completed) {
            self.fire_rating_update();
        }

        Ok(success)
    }

    /// Spawn a background task that loads the 4 seats' user_ids, looks up
    /// each registered player's current rating, computes the Glicko-2
    /// update, and writes the new ratings back. Anon / bot seats are
    /// treated as default-rated opponents but don't get a stored update
    /// of their own (no row to write to).
    fn fire_rating_update(&self) {
        let Some(db) = self.db.clone() else { return };
        let game_id = self.game_id;
        let team_a_score = self.game.get_team_a_score().ok().copied().unwrap_or(0);
        let team_b_score = self.game.get_team_b_score().ok().copied().unwrap_or(0);
        let team_a_won = team_a_score > team_b_score;
        tokio::spawn(async move {
            let _ = tokio::task::spawn_blocking(move || {
                apply_glicko_update(&db, game_id, team_a_won);
            })
            .await;
        });
    }

    fn handle_set_player_name(
        &mut self,
        player_id: Uuid,
        name: Option<String>,
    ) -> Result<(), GameManagerError> {
        self.game.set_player_name(player_id, name)?;
        self.persist_async();
        let state_response = self.build_state_response();
        self.broadcast(GameEvent::StateChanged { seq: 0, state: state_response });
        Ok(())
    }

    fn handle_timer_fired(&mut self, generation: u64) {
        // Generation check: discard messages from timers that have since
        // been cancelled by a subsequent transition (or restarted with a
        // new generation). This replaces the cross-lock race-guard at the
        // old `handle_timeout`.
        match &self.active_timer {
            Some(t) if t.generation == generation => {}
            _ => return,
        }
        let expected_player_index =
            self.active_timer.as_ref().map(|t| t.expected_player_index).unwrap_or(0);
        let player_idx = self.game.get_current_player_index_num();
        if expected_player_index != player_idx {
            return;
        }
        let is_first_round_betting = self.game.is_first_round_betting();
        let current_state = self.game.get_state().clone();

        // Zero the timed-out player's clock and clear the timer slot. The
        // task that just sent us this message is already done; no abort
        // needed.
        if let Some(clocks) = self.game.get_player_clocks_mut() {
            clocks.remaining_ms[player_idx] = 0;
        }
        self.active_timer = None;

        if is_first_round_betting {
            self.game.set_state(State::Aborted);
            self.game.set_turn_started_at_epoch_ms(None);
            self.persist_async();
            self.broadcast(GameEvent::GameAborted {
                seq: 0,
                game_id: self.game_id,
                reason: "Player timed out during first round betting".to_string(),
            });
            return;
        }

        let transition = match current_state {
            State::Betting(_) => Some(GameTransition::Bet(1)),
            State::Trick(_) => {
                let cards = self.game.get_legal_cards().ok();
                cards.and_then(|cards| {
                    if cards.is_empty() {
                        None
                    } else {
                        let mut rng = rand::thread_rng();
                        cards.choose(&mut rng).copied().map(GameTransition::Card)
                    }
                })
            }
            _ => None,
        };
        if let Some(t) = transition {
            let _ = self.apply_one_transition(t, true);
            while let Some(ai_trans) = self.next_ai_transition() {
                let _ = self.apply_one_transition(ai_trans, false);
            }
        }
    }

    // -- helpers --------------------------------------------------------

    /// Serialize `self.game` and hand the bytes to a `spawn_blocking`
    /// worker for the SQL write. Returns immediately so the actor's
    /// mailbox isn't stalled by rusqlite I/O.
    ///
    /// Durability is best-effort: a process crash between the in-memory
    /// transition and the row update loses the most recent change, but
    /// `with_graceful_shutdown` drains spawned tasks before the runtime
    /// exits, so SIGTERM / Ctrl+C still flush pending persists.
    fn persist_async(&self) {
        let Some(db) = self.db.clone() else { return };
        let game_id = self.game_id;
        let json = match serde_json::to_string(&self.game) {
            Ok(j) => j,
            Err(e) => {
                error!(game_id = %game_id, error = %e, "failed to serialize game for persist");
                return;
            }
        };
        tokio::spawn(async move {
            let res = tokio::task::spawn_blocking(move || db.update_game_serialized(game_id, json)).await;
            match res {
                Ok(Ok(())) => {}
                Ok(Err(e)) => error!(game_id = %game_id, error = %e, "persist update failed"),
                Err(e) => error!(game_id = %game_id, error = %e, "persist task join failed"),
            }
        });
    }

    fn cancel_timer(&mut self) -> (u64, Option<usize>) {
        if let Some(timer) = self.active_timer.take() {
            let elapsed = timer.turn_started_at.elapsed().as_millis() as u64;
            let idx = timer.expected_player_index;
            // `Drop for ActiveTimer` aborts the JoinHandle.
            drop(timer);
            (elapsed, Some(idx))
        } else {
            (0, None)
        }
    }

    fn start_timer(&mut self, player_index: usize, remaining_ms: u64) {
        self.timer_generation += 1;
        let generation = self.timer_generation;
        let self_tx = self.self_tx.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(remaining_ms)).await;
            let _ = self_tx.send(GameCmd::TimerFired { generation });
        });
        self.active_timer = Some(ActiveTimer {
            handle,
            generation,
            turn_started_at: tokio::time::Instant::now(),
            remaining_at_turn_start_ms: remaining_ms,
            expected_player_index: player_index,
        });
    }

    fn restart_timer_if_in_progress(&mut self) {
        if self.game.get_timer_config().is_none() {
            return;
        }
        match self.game.get_state() {
            State::Betting(_) | State::Trick(_) => {}
            _ => return,
        }
        let (Some(epoch_ms), Some(clocks)) = (
            self.game.get_turn_started_at_epoch_ms(),
            self.game.get_player_clocks(),
        ) else {
            return;
        };
        let player_idx = self.game.get_current_player_index_num();
        let now = epoch_ms_now();
        let elapsed = now.saturating_sub(epoch_ms);
        let remaining = clocks.remaining_ms[player_idx].saturating_sub(elapsed);
        self.start_timer(player_idx, remaining);
    }

    fn broadcast(&mut self, mut event: GameEvent) {
        let seq = self.next_seq;
        self.next_seq += 1;
        // Re-stamp seq inside whichever variant we got handed (callers
        // pass `seq: 0` placeholder).
        match &mut event {
            GameEvent::StateChanged { seq: s, .. } => *s = seq,
            GameEvent::GameAborted { seq: s, .. } => *s = seq,
            GameEvent::ChatMessage { seq: s, .. } => *s = seq,
        }
        self.recent.push_back(event.clone());
        while self.recent.len() > RECENT_CAP {
            self.recent.pop_front();
        }
        let _ = self.sender.send(event);
    }

    fn next_ai_transition(&self) -> Option<GameTransition> {
        let cfg = self.ai_config.as_ref()?;
        match self.game.get_state() {
            State::Betting(_) | State::Trick(_) => {}
            _ => return None,
        }
        let player_idx = self.game.get_current_player_index_num();
        if !cfg.ai_players.contains(&player_idx) {
            return None;
        }
        match self.game.get_state() {
            State::Betting(_) => Some(GameTransition::Bet(cfg.strategy.choose_bet(&self.game, player_idx))),
            State::Trick(_) => Some(GameTransition::Card(cfg.strategy.choose_card(&self.game, player_idx))),
            _ => None,
        }
    }

    fn build_state_response(&self) -> GameStateResponse {
        let names = self.game.get_player_names();
        let timer_config = self.game.get_timer_config().copied();

        let (player_clocks_ms, active_player_clock_ms) =
            if let Some(clocks) = self.game.get_player_clocks() {
                let mut clocks_snapshot = clocks.remaining_ms;
                let mut active_clock = None;
                if let Some(timer) = &self.active_timer {
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
            game_id: self.game_id,
            short_id: spades::uuid_to_short_id(self.game_id),
            state: self.game.get_state().clone(),
            team_a_score: self.game.get_team_a_score().ok().copied(),
            team_b_score: self.game.get_team_b_score().ok().copied(),
            team_a_bags: self.game.get_team_a_bags().ok().copied(),
            team_b_bags: self.game.get_team_b_bags().ok().copied(),
            current_player_id: self.game.get_current_player_id().ok().copied(),
            player_names: [
                PlayerNameEntry { player_id: names[0].0, name: names[0].1.map(String::from) },
                PlayerNameEntry { player_id: names[1].0, name: names[1].1.map(String::from) },
                PlayerNameEntry { player_id: names[2].0, name: names[2].1.map(String::from) },
                PlayerNameEntry { player_id: names[3].0, name: names[3].1.map(String::from) },
            ],
            timer_config,
            player_clocks_ms,
            active_player_clock_ms,
            table_cards: self.game.get_current_trick_cards().ok().cloned(),
            player_bets: self.game.get_player_bets(),
            player_tricks_won: self.game.get_player_tricks_won(),
            last_trick_winner_id: self.game.get_last_trick_winner_id(),
            last_completed_trick: self.game.get_last_completed_trick().cloned(),
        }
    }

    fn build_subscription(&self, since: Option<u64>) -> Subscription {
        let current_seq = self.next_seq;
        let rx = self.sender.subscribe();
        let initial_state = self.build_state_response();
        let catch_up = match since {
            None => None,
            Some(n) if n == current_seq => Some(Vec::new()),
            Some(n) if n > current_seq => None,
            Some(n) => {
                let front_seq = self.recent.front().map(event_seq);
                match front_seq {
                    Some(fs) if fs <= n => Some(
                        self.recent
                            .iter()
                            .filter(|e| {
                                let s = event_seq(e);
                                s >= n && s < current_seq
                            })
                            .cloned()
                            .collect(),
                    ),
                    _ => None,
                }
            }
        };
        Subscription { rx, current_seq, initial_state, catch_up }
    }
}

/// Pull the 4 seats, read each registered user's current rating, compute
/// the Glicko-2 update for each, and persist. Failures log but never
/// propagate — rating updates are best-effort. Anon / bot seats are
/// treated as default-rated opponents but don't get a stored update.
fn apply_glicko_update(db: &SqliteStore, game_id: Uuid, team_a_won: bool) {
    let mut seats: [Option<crate::auth::game_seats::SeatRow>; 4] = [None, None, None, None];
    for (i, slot) in seats.iter_mut().enumerate() {
        *slot = match db.game_seat(game_id, i as i32) {
            Ok(s) => s,
            Err(e) => {
                error!(game_id = %game_id, seat = i, error = %e, "rating update: seat lookup failed");
                return;
            }
        };
    }
    let mut current: [Rating; 4] = [DEFAULT_RATING; 4];
    for (slot, seat) in current.iter_mut().zip(seats.iter()) {
        if let Some(seat) = seat
            && let Some(user_id) = seat.user_id
            && let Ok(Some(r)) = db.get_user_rating(user_id)
        {
            *slot = r;
        }
    }
    // Spades partnership: seats 0+2 are team A, 1+3 are team B.
    for (i, seat) in seats.iter().enumerate() {
        let Some(seat) = seat else { continue };
        let Some(user_id) = seat.user_id else { continue };
        let on_team_a = (i % 2) == 0;
        let opponents_idx: [usize; 2] = if on_team_a { [1, 3] } else { [0, 2] };
        let outcome = if on_team_a == team_a_won { 1.0 } else { 0.0 };
        let opp_slate = [
            (current[opponents_idx[0]], outcome),
            (current[opponents_idx[1]], outcome),
        ];
        let new_rating = ratings::update(current[i], &opp_slate);
        if let Err(e) = db.set_user_rating(user_id, &new_rating) {
            error!(game_id = %game_id, user_id = %user_id, error = %e, "rating update: write failed");
        }
    }
}
