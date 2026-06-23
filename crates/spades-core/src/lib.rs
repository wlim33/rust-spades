//! This crate provides an implementation of the four person card game, [spades](https://www.pagat.com/auctionwhist/spades.html).
//! ## Example usage
//! ```
//! use spades::{Game, GameTransition, State};
//! use rand::seq::IndexedRandom;
//!
//! let mut g = Game::new(
//!     uuid::Uuid::new_v4(),
//!     [uuid::Uuid::new_v4(); 4],
//!     100,
//!     None,
//! );
//! g.play(GameTransition::Start).unwrap();
//! let mut rng = rand::rng();
//! while *g.get_state() != State::Completed {
//!     if let State::Trick(_) = *g.get_state() {
//!         let legal = g.get_legal_cards().unwrap();
//!         let card = *legal.choose(&mut rng).unwrap();
//!         g.play(GameTransition::Card(card)).unwrap();
//!     } else {
//!         g.play(GameTransition::Bet(3)).unwrap();
//!     }
//! }
//! assert_eq!(*g.get_state(), State::Completed);
//! ```

#![allow(clippy::large_enum_variant)]

pub mod ai;
mod cards;
mod game_state;
mod result;
mod rules;
mod scoring;
pub mod transcript;

#[cfg(test)]
mod tests;

pub use cards::*;
pub use game_state::*;
pub use result::*;
use sqids::Sqids;
use std::sync::OnceLock;
use uuid::Uuid;

fn sqids_instance() -> &'static Sqids {
    static SQIDS: OnceLock<Sqids> = OnceLock::new();
    SQIDS.get_or_init(|| {
        Sqids::builder()
            .min_length(6)
            .build()
            .expect("valid sqids config")
    })
}

/// Split a [`Uuid`] into its big-endian `(high, low)` `u64` halves.
fn uuid_to_pair(uuid: Uuid) -> [u64; 2] {
    let b = uuid.as_bytes();
    [
        u64::from_be_bytes(b[0..8].try_into().unwrap()),
        u64::from_be_bytes(b[8..16].try_into().unwrap()),
    ]
}

/// Reassemble a [`Uuid`] from its big-endian `(high, low)` `u64` halves.
fn uuid_from_pair(high: u64, low: u64) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[0..8].copy_from_slice(&high.to_be_bytes());
    bytes[8..16].copy_from_slice(&low.to_be_bytes());
    Uuid::from_bytes(bytes)
}

/// Encode a [`Uuid`] as a short, URL-safe id (sqids, min length 6). Inverse of [`short_id_to_uuid`].
pub fn uuid_to_short_id(uuid: Uuid) -> String {
    sqids_instance()
        .encode(&uuid_to_pair(uuid))
        .expect("sqids encode")
}

/// Decode a short id from [`uuid_to_short_id`] back into a [`Uuid`], or `None` if malformed.
pub fn short_id_to_uuid(short_id: &str) -> Option<Uuid> {
    match sqids_instance().decode(short_id)[..] {
        [high, low] => Some(uuid_from_pair(high, low)),
        _ => None,
    }
}

/// Encode a `(game_id, player_id)` pair into a single short, URL-safe token. Inverse of [`decode_player_url`].
pub fn encode_player_url(game_id: Uuid, player_id: Uuid) -> String {
    let [g_hi, g_lo] = uuid_to_pair(game_id);
    let [p_hi, p_lo] = uuid_to_pair(player_id);
    sqids_instance()
        .encode(&[g_hi, g_lo, p_hi, p_lo])
        .expect("sqids encode")
}

/// Decode a token from [`encode_player_url`] into `(game_id, player_id)`, or `None` if malformed.
pub fn decode_player_url(s: &str) -> Option<(Uuid, Uuid)> {
    match sqids_instance().decode(s)[..] {
        [g_hi, g_lo, p_hi, p_lo] => Some((uuid_from_pair(g_hi, g_lo), uuid_from_pair(p_hi, p_lo))),
        _ => None,
    }
}

/// The primary way to interface with a spades game. Used as an argument to [Game::play](struct.Game.html#method.play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameTransition {
    Bet(i32),
    Card(Card),
    Start,
    /// Abandon the game, moving it to [`State::Aborted`]. Rejected if the game
    /// has already reached a terminal state (`Completed`/`Aborted`).
    Abort,
}

/// Fischer increment timer configuration (X+Y: X minutes initial, Y seconds increment per move).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(oasgen::OaSchema))]
pub struct TimerConfig {
    pub initial_time_secs: u64,
    pub increment_secs: u64,
}

/// Remaining clock time for each player in milliseconds.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlayerClocks {
    pub remaining_ms: [u64; 4],
}

/// Primary game state. A thin facade over [`trick_engine::Game`] configured with
/// the [`crate::rules::Spades`] ruleset; this crate owns only the timer/clock
/// bookkeeping and the "last completed trick" convenience snapshot. The public
/// API is unchanged from the pre-engine implementation.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Game {
    inner: trick_engine::Game,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timer_config: Option<TimerConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    player_clocks: Option<PlayerClocks>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    turn_started_at_epoch_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_trick_winner: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_completed_trick: Option<[cards::Card; 4]>,
}

/// Map an engine `StepOutcome` onto the facade's `TransitionSuccess`. Trick and
/// round completion both surface as `Trick` (the historical API never
/// distinguished them).
fn map_outcome(o: trick_engine::StepOutcome) -> TransitionSuccess {
    use trick_engine::StepOutcome as O;
    match o {
        O::Started => TransitionSuccess::Start,
        O::Bid => TransitionSuccess::Bet,
        O::BidComplete => TransitionSuccess::BetComplete,
        O::PlayCard => TransitionSuccess::PlayCard,
        O::TrickComplete | O::RoundComplete => TransitionSuccess::Trick,
        O::GameOver => TransitionSuccess::GameOver,
        O::Aborted => TransitionSuccess::Aborted,
    }
}

impl Game {
    /// Create a new game for the four `player_ids` (seat order A, B, C, D), played to
    /// `max_points`, with an optional Fischer-increment [`TimerConfig`]. The game begins in
    /// [`State::NotStarted`]; call [`Game::play`] with [`GameTransition::Start`] to deal and bet.
    pub fn new(
        id: Uuid,
        player_ids: [Uuid; 4],
        max_points: i32,
        timer_config: Option<TimerConfig>,
    ) -> Game {
        let player_clocks = timer_config.map(|tc| PlayerClocks {
            remaining_ms: [tc.initial_time_secs * 1000; 4],
        });
        let rules = Box::new(crate::rules::Spades::new(max_points));
        Game {
            inner: trick_engine::Game::new(id, player_ids.to_vec(), rules),
            timer_config,
            player_clocks,
            turn_started_at_epoch_ms: None,
            last_trick_winner: None,
            last_completed_trick: None,
        }
    }

    /// Borrow the concrete spades ruleset out of the engine. Infallible: a
    /// `Game` is always constructed with `Spades`.
    fn spades(&self) -> &crate::rules::Spades {
        self.inner
            .rules_as::<crate::rules::Spades>()
            .expect("spades game always carries the Spades ruleset")
    }

    /// The game's unique id.
    pub fn get_id(&self) -> &Uuid {
        self.inner.id()
    }

    /// See [`State`](enum.State.html)
    pub fn get_state(&self) -> &State {
        self.inner.state()
    }

    /// `Ok` once the game has started — i.e. any state other than [`State::NotStarted`].
    fn require_started(&self) -> Result<(), GetError> {
        match self.inner.state() {
            State::NotStarted => Err(GetError::GameNotStarted),
            _ => Ok(()),
        }
    }

    /// `Ok` only while a hand is in progress (Betting or Trick), distinguishing
    /// not-yet-started from already-finished.
    fn require_active(&self) -> Result<(), GetError> {
        match self.inner.state() {
            State::NotStarted => Err(GetError::GameNotStarted),
            State::Completed | State::Aborted => Err(GetError::GameCompleted),
            State::Bidding(_) | State::Trick(_) => Ok(()),
        }
    }

    /// Team A's (seats 0 & 2) cumulative score. `Err` before the game has started.
    pub fn get_team_a_score(&self) -> Result<i32, GetError> {
        self.require_started()?;
        Ok(self.spades().scoring().team_a.cumulative_points)
    }

    /// Team B's (seats 1 & 3) cumulative score. `Err` before the game has started.
    pub fn get_team_b_score(&self) -> Result<i32, GetError> {
        self.require_started()?;
        Ok(self.spades().scoring().team_b.cumulative_points)
    }

    /// Team A's accumulated bags (overtricks). `Err` before the game has started.
    pub fn get_team_a_bags(&self) -> Result<i32, GetError> {
        self.require_started()?;
        Ok(self.spades().scoring().team_a.bags)
    }

    /// Team B's accumulated bags (overtricks). `Err` before the game has started.
    pub fn get_team_b_bags(&self) -> Result<i32, GetError> {
        self.require_started()?;
        Ok(self.spades().scoring().team_b.bags)
    }

    /// Returns `GetError` when the current game is not in the Betting or Trick stages.
    pub fn get_current_player_id(&self) -> Result<Uuid, GetError> {
        self.require_active()?;
        Ok(self.inner.player_id(self.inner.current_seat()))
    }

    /// A seat's hand as sorted spades `Card`s. The engine deals unsorted; the
    /// legacy spades API always exposed hands in canonical order (suit, then
    /// rank), which callers/tests rely on (e.g. taking `hand[0]` as the lowest
    /// card). Sorting the *view* doesn't affect engine play, which matches cards
    /// by identity, not position.
    fn hand_of(&self, seat: usize) -> Vec<Card> {
        let mut hand: Vec<Card> = self
            .inner
            .hand(seat)
            .iter()
            .filter_map(cards::from_tn)
            .collect();
        hand.sort();
        hand
    }

    /// Returns a `GetError::InvalidUuid` if the game does not contain a player with the given `Uuid`.
    pub fn get_hand_by_player_id(&self, player_id: Uuid) -> Result<Vec<Card>, GetError> {
        for seat in 0..4 {
            if self.inner.player_id(seat) == player_id {
                return Ok(self.hand_of(seat));
            }
        }
        Err(GetError::InvalidUuid)
    }

    /// The hand of the player whose turn it is. `Err` unless the game is in the Betting or Trick stage.
    pub fn get_current_hand(&self) -> Result<Vec<Card>, GetError> {
        self.require_active()?;
        Ok(self.hand_of(self.inner.current_seat()))
    }

    /// The suit led in the current trick, or `None` if no card has been led yet. Only valid in the Trick stage.
    pub fn get_leading_suit(&self) -> Result<Option<Suit>, GetError> {
        match self.inner.state() {
            State::NotStarted => Err(GetError::GameNotStarted),
            State::Completed => Err(GetError::GameCompleted),
            State::Trick(_) => {
                let leader = self.inner.trick_leader();
                Ok(self.inner.current_trick()[leader]
                    .as_ref()
                    .and_then(cards::from_tn)
                    .map(|c| c.suit))
            }
            _ => Err(GetError::Unknown),
        }
    }

    /// Returns the cards currently on the table; each slot is `None` if that player
    /// hasn't yet played this trick. Only available in the Trick stage.
    pub fn get_current_trick_cards(&self) -> Result<[Option<cards::Card>; 4], GetError> {
        match self.inner.state() {
            State::NotStarted => Err(GetError::GameNotStarted),
            State::Completed | State::Aborted => Err(GetError::GameCompleted),
            State::Bidding(_) => Err(GetError::Unknown),
            State::Trick(_) => {
                let trick = self.inner.current_trick();
                Ok(std::array::from_fn(|i| {
                    trick[i].as_ref().and_then(cards::from_tn)
                }))
            }
        }
    }

    /// The ids of the two players on the winning team. `Err` unless the game has completed.
    pub fn get_winner_ids(&self) -> Result<(Uuid, Uuid), GetError> {
        match self.inner.state() {
            State::Completed => {
                let s = self.spades().scoring();
                if s.team_a.cumulative_points > s.team_b.cumulative_points {
                    Ok((self.inner.player_id(0), self.inner.player_id(2)))
                } else if s.team_b.cumulative_points > s.team_a.cumulative_points {
                    Ok((self.inner.player_id(1), self.inner.player_id(3)))
                } else {
                    // A tie at State::Completed is reachable only when the game
                    // ends via the loss floor or round cap (max_points keeps
                    // playing on a tie). No production code calls this on a tied
                    // game; the server's rating path compares scores directly.
                    Err(GetError::GameNotCompleted)
                }
            }
            _ => Err(GetError::GameNotCompleted),
        }
    }

    /// The primary function used to progress the game state. The first `GameTransition` argument must always be
    /// [`GameTransition::Start`](enum.GameTransition.html#variant.Start). The stages and player rotations are managed
    /// internally. The order of `GameTransition` arguments should be:
    ///
    /// Start -> Bet * 4 -> Card * 13 -> Bet * 4 -> Card * 13 -> Bet * 4 -> ...
    pub fn play(&mut self, entry: GameTransition) -> Result<TransitionSuccess, TransitionError> {
        self.last_completed_trick = None;
        let action = match entry {
            GameTransition::Start => trick_engine::Action::Start,
            GameTransition::Bet(b) => trick_engine::Action::Bid(b),
            GameTransition::Card(c) => trick_engine::Action::Play(cards::to_tn(c)),
            GameTransition::Abort => trick_engine::Action::Abort,
        };
        match self.inner.step(action) {
            Ok(outcome) => {
                // Capture the just-completed trick / its winner for the getters.
                // After a trick the engine clears the table and pushes the
                // completed trick to history; its winner becomes the new leader.
                if matches!(
                    outcome,
                    trick_engine::StepOutcome::TrickComplete
                        | trick_engine::StepOutcome::RoundComplete
                        | trick_engine::StepOutcome::GameOver
                ) {
                    if let Some(last) = self.inner.history().last()
                        && last.iter().all(|c| c.is_some())
                    {
                        let arr: [Card; 4] = std::array::from_fn(|i| {
                            cards::from_tn(last[i].as_ref().unwrap())
                                .expect("history holds only spades cards")
                        });
                        self.last_completed_trick = Some(arr);
                        self.last_trick_winner = Some(self.inner.trick_leader());
                    }
                    // A round boundary clears the inter-round winner the way
                    // the old engine did (winner is meaningful only within a round).
                    // On GameOver the final trick's winner must be retained so that
                    // get_last_trick_winner_id() and get_last_completed_trick() are
                    // consistent in the completed-game snapshot.
                    if matches!(outcome, trick_engine::StepOutcome::RoundComplete) {
                        self.last_trick_winner = None;
                    }
                }
                Ok(map_outcome(outcome))
            }
            Err(e) => Err(self.map_step_error(e, entry)),
        }
    }

    /// Translate the engine's coarse error into the precise spades variant the
    /// public API has always returned. The engine says "no"; spades explains why.
    fn map_step_error(&self, e: trick_engine::StepError, entry: GameTransition) -> TransitionError {
        use trick_engine::StepError as E;
        match e {
            E::NotStarted => TransitionError::NotStarted,
            E::AlreadyStarted => TransitionError::AlreadyStarted,
            E::Completed => TransitionError::CompletedGame,
            E::IllegalBid => TransitionError::InvalidBet,
            E::CardNotInHand => TransitionError::CardNotInHand,
            E::WrongPhase => match entry {
                GameTransition::Bet(_) => TransitionError::BetInTrickStage,
                GameTransition::Card(_) => TransitionError::CardInBettingStage,
                _ => TransitionError::CompletedGame,
            },
            E::IllegalPlay => self.explain_illegal_play(entry),
        }
    }

    /// Re-derive `SpadesNotBroken` vs `CardIncorrectSuit` for an in-hand-but-illegal
    /// card, matching the historical engine behavior. Reached only in the Trick
    /// phase with a `Card` transition.
    fn explain_illegal_play(&self, entry: GameTransition) -> TransitionError {
        let GameTransition::Card(card) = entry else {
            return TransitionError::CardIncorrectSuit;
        };
        let seat = self.inner.current_seat();
        let hand: Vec<Card> = self
            .inner
            .hand(seat)
            .iter()
            .filter_map(cards::from_tn)
            .collect();
        let leader = self.inner.trick_leader();
        let leading = self.inner.current_trick()[leader]
            .as_ref()
            .and_then(cards::from_tn)
            .map(|c| c.suit);
        match leading {
            // Leading the trick (no card down yet): the only illegal lead is an
            // unbroken spade while non-spades remain.
            None => TransitionError::SpadesNotBroken,
            // Following: illegal because a card of the led suit was held but not played.
            Some(ls) if card.suit != ls && hand.iter().any(|c| c.suit == ls) => {
                TransitionError::CardIncorrectSuit
            }
            _ => TransitionError::CardIncorrectSuit,
        }
    }

    /// Set (or clear, with `None`) a player's display name. `Err` if no player matches `player_id`.
    pub fn set_player_name(
        &mut self,
        player_id: Uuid,
        name: Option<String>,
    ) -> Result<(), GetError> {
        for seat in 0..4 {
            if self.inner.player_id(seat) == player_id {
                self.inner.player_mut(seat).name = name;
                return Ok(());
            }
        }
        Err(GetError::InvalidUuid)
    }

    /// Each seat's `(id, display name)` in seat order.
    pub fn get_player_names(&self) -> [(Uuid, Option<&str>); 4] {
        std::array::from_fn(|i| (self.inner.player_id(i), self.inner.player_name(i)))
    }

    /// The Fischer-increment timer config, if this game was created with one.
    pub fn get_timer_config(&self) -> Option<&TimerConfig> {
        self.timer_config.as_ref()
    }

    /// Each player's remaining clock, if this game uses timers.
    pub fn get_player_clocks(&self) -> Option<&PlayerClocks> {
        self.player_clocks.as_ref()
    }

    /// Mutable access to the player clocks, if this game uses timers (server debits time here).
    pub fn get_player_clocks_mut(&mut self) -> Option<&mut PlayerClocks> {
        self.player_clocks.as_mut()
    }

    /// The 0-based seat index of the player whose turn it is.
    pub fn get_current_player_index_num(&self) -> usize {
        self.inner.current_seat()
    }

    /// Returns true if the game is in the first round's betting phase (round 0, Betting state).
    pub fn is_first_round_betting(&self) -> bool {
        self.inner.round() == 0 && matches!(self.inner.state(), State::Bidding(_))
    }

    /// Wall-clock time (epoch ms) the current turn began, if tracked (server-set for timed games).
    pub fn get_turn_started_at_epoch_ms(&self) -> Option<u64> {
        self.turn_started_at_epoch_ms
    }

    /// Record when the current turn began (epoch ms); used by the server for clock accounting.
    pub fn set_turn_started_at_epoch_ms(&mut self, epoch_ms: Option<u64>) {
        self.turn_started_at_epoch_ms = epoch_ms;
    }

    /// Each seat's bet for the current round, or `None` before the game has started.
    pub fn get_player_bets(&self) -> Option<[i32; 4]> {
        match self.inner.state() {
            State::NotStarted => None,
            // The in-progress round's bids live in the engine; `bets_placed` is
            // only written at round end (`finalize_round`).
            _ => Some(self.inner.bids().try_into().expect("4 seats")),
        }
    }

    /// Each seat's tricks won so far this round, or `None` before the game has started.
    pub fn get_player_tricks_won(&self) -> Option<[i32; 4]> {
        match self.inner.state() {
            State::NotStarted => None,
            _ => Some(self.inner.tricks_won().try_into().expect("4 seats")),
        }
    }

    /// The id of the player who won the most recently completed trick, or `None`
    /// between rounds and before any trick completes.
    pub fn get_last_trick_winner_id(&self) -> Option<Uuid> {
        // `last_trick_winner` is always a valid seat by construction; `.min(3)`
        // defensively guards against an out-of-range index from a corrupt
        // deserialized row (prefer a wrong-but-safe id over a panic here).
        self.last_trick_winner
            .map(|idx| self.inner.player_id(idx.min(3)))
    }

    /// The four cards of the most recently completed trick, or `None` if none has completed this round.
    pub fn get_last_completed_trick(&self) -> Option<&[cards::Card; 4]> {
        self.last_completed_trick.as_ref()
    }

    /// Set the game state directly. Crate-internal escape hatch for transcript
    /// replay and tests; external callers abort via [`GameTransition::Abort`].
    pub(crate) fn set_state(&mut self, state: State) {
        self.inner.set_state(state);
    }

    /// Returns the list of legal cards the current player can play.
    /// Only valid in the Trick state.
    pub fn get_legal_cards(&self) -> Result<Vec<Card>, GetError> {
        match self.inner.state() {
            State::Trick(_) => Ok(self
                .inner
                .legal_plays()
                .iter()
                .filter_map(cards::from_tn)
                .collect()),
            _ => Err(GetError::Unknown),
        }
    }

    /// Max points configured at game creation.
    pub fn get_max_points(&self) -> i32 {
        self.spades().scoring().config.max_points
    }

    /// All trick slots, one per trick. For round R the slots live at indices
    /// 13*R .. 13*(R+1). The final slot may be partially filled (current trick).
    /// Empty trailing slot during betting between rounds is intentional.
    ///
    /// Owned (one `[Option<Card>; 4]` per trick) because the engine stores tricks
    /// as `Vec<Option<TnCard>>` and the spades view converts on read.
    ///
    /// The engine's `history()` holds only *completed* tricks. The legacy spades
    /// view additionally carried a trailing slot for the in-progress trick (and a
    /// fresh empty slot during between-round betting), so every non-terminal state
    /// gets that trailing slot appended here — keeping the `13*round` indexing and
    /// length expectations the transcript adapter and tests rely on. A completed
    /// game has no trailing slot (the round-ending trick never pushed one).
    pub fn get_history(&self) -> Vec<[Option<cards::Card>; 4]> {
        let convert = |trick: &[Option<trick_notation::Card>]| -> [Option<cards::Card>; 4] {
            std::array::from_fn(|i| trick[i].as_ref().and_then(cards::from_tn))
        };
        let mut out: Vec<[Option<cards::Card>; 4]> =
            self.inner.history().iter().map(|t| convert(t)).collect();
        if !matches!(self.inner.state(), State::Completed) {
            out.push(convert(self.inner.current_trick()));
        }
        out
    }

    /// All bets per round in seat order. `bets[R][s]` is seat `s`'s bet for round
    /// `R`. The engine's `finalize_round` only writes a round's bets at round end,
    /// so the current (in-progress) round's slot is overwritten with the live
    /// engine bids — required for transcript correctness. `Completed` games are
    /// left untouched (every round is already finalized, and the live engine bids
    /// would otherwise leak the final round's bids into the trailing slot);
    /// `Aborted` games ARE patched, because the round they aborted in was never
    /// finalized and its bids live only in the engine.
    pub fn get_all_bets(&self) -> Vec<[i32; 4]> {
        let mut bets = self.spades().scoring().bets_placed.clone();
        let round = self.inner.round();
        if matches!(
            self.inner.state(),
            State::Bidding(_) | State::Trick(_) | State::Aborted
        ) {
            let live: [i32; 4] = self.inner.bids().try_into().expect("4 seats");
            if round < bets.len() {
                bets[round] = live;
            } else {
                bets.push(live);
            }
        }
        bets
    }

    /// Current 0-based round index. Sourced from the spades scoring state (not
    /// the engine's `round()`), because the legacy API incremented the round
    /// counter on *every* finalized round — including the game-ending one — and
    /// the transcript adapter's `13*round` indexing and round-count emission
    /// depend on that. (The engine leaves its own `round` at the last value when
    /// the game ends, so the two diverge by one only for a completed game.)
    pub fn get_round_index(&self) -> usize {
        self.spades().scoring().round
    }

    /// True when the game is in (or just finished) a betting phase rather than
    /// a trick phase. Combined with `get_state()` this disambiguates Aborted
    /// games.
    pub fn is_in_betting_stage(&self) -> bool {
        self.spades().scoring().in_betting_stage
    }

    /// Override each player's hand with the given cards (used by transcript replay
    /// to seed the engine with the hands declared in the transcript rather than the
    /// randomly-dealt ones).  The caller is responsible for correctness; this does
    /// not validate that the supplied cards form a legal deal.
    pub(crate) fn override_hands(&mut self, hands: [Vec<Card>; 4]) {
        for (i, hand) in hands.into_iter().enumerate() {
            self.inner.player_mut(i).hand = hand.into_iter().map(cards::to_tn).collect();
        }
    }

    /// Set a single seat's hand. Crate-internal test helper (the old tests
    /// poked `players[i].hand` directly); replaces that field access now that
    /// hands live in the engine.
    #[cfg(test)]
    pub(crate) fn set_player_hand(&mut self, seat: usize, hand: Vec<Card>) {
        self.inner.player_mut(seat).hand = hand.into_iter().map(cards::to_tn).collect();
    }

    /// Mutable access to the spades scoring state. Crate-internal test helper
    /// for seeding terminal scores when exercising `get_winner_ids`.
    #[cfg(test)]
    pub(crate) fn scoring_mut(&mut self) -> &mut crate::scoring::Scoring {
        self.inner
            .rules_as_mut::<crate::rules::Spades>()
            .expect("spades game always carries the Spades ruleset")
            .scoring_mut()
    }
}

/// The engine `Game` (and its `Box<dyn Ruleset>`) doesn't implement `Debug`, so
/// the facade can't derive it. A summary impl keeps `Result::unwrap`/`expect`
/// and assertion diagnostics working without exposing the full inner state.
impl std::fmt::Debug for Game {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Game")
            .field("id", self.inner.id())
            .field("state", self.inner.state())
            .field("round", &self.inner.round())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod facade_tests {
    use super::*;
    use crate::cards::{Card, Rank, Suit};
    use uuid::Uuid;

    fn ids() -> [Uuid; 4] {
        [
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Uuid::from_u128(3),
            Uuid::from_u128(4),
        ]
    }

    #[test]
    fn full_game_drives_to_completion_through_facade() {
        let mut g = Game::new(Uuid::from_u128(9), ids(), 50, None);
        g.play(GameTransition::Start).unwrap();
        while *g.get_state() != State::Completed {
            match g.get_state() {
                State::Bidding(_) => {
                    g.play(GameTransition::Bet(3)).unwrap();
                }
                State::Trick(_) => {
                    let legal = g.get_legal_cards().unwrap();
                    g.play(GameTransition::Card(legal[0])).unwrap();
                }
                _ => unreachable!(),
            }
        }
        assert_eq!(*g.get_state(), State::Completed);
        assert!(g.get_team_a_score().is_ok());
    }

    #[test]
    fn spades_not_broken_error_is_preserved() {
        // Mirrors `cannot_lead_spade_before_broken_when_non_spades_available` in
        // src/tests, but exercises the facade's IllegalPlay re-derivation: seat 0
        // holds a club and a spade on lead with spades unbroken, so leading the
        // spade must surface as the precise `SpadesNotBroken` variant.
        let mut g = Game::new(Uuid::from_u128(42), ids(), 500, None);
        g.play(GameTransition::Start).unwrap();
        g.set_player_hand(
            0,
            vec![
                Card {
                    suit: Suit::Club,
                    rank: Rank::Five,
                },
                Card {
                    suit: Suit::Spade,
                    rank: Rank::Ace,
                },
            ],
        );
        g.set_player_hand(
            1,
            vec![Card {
                suit: Suit::Club,
                rank: Rank::Two,
            }],
        );
        g.set_player_hand(
            2,
            vec![Card {
                suit: Suit::Club,
                rank: Rank::Three,
            }],
        );
        g.set_player_hand(
            3,
            vec![Card {
                suit: Suit::Club,
                rank: Rank::Four,
            }],
        );
        for _ in 0..4 {
            g.play(GameTransition::Bet(3)).unwrap();
        }
        assert!(matches!(g.get_state(), State::Trick(0)));
        assert_eq!(
            g.play(GameTransition::Card(Card {
                suit: Suit::Spade,
                rank: Rank::Ace
            })),
            Err(TransitionError::SpadesNotBroken)
        );
    }

    #[test]
    fn completed_game_has_consistent_final_trick_snapshot() {
        // Regression: play() used to null last_trick_winner on both RoundComplete
        // AND GameOver, producing an inconsistent snapshot where
        // get_last_completed_trick() returned Some(...) but
        // get_last_trick_winner_id() returned None on a finished game.
        let mut g = Game::new(Uuid::from_u128(9), ids(), 50, None);
        g.play(GameTransition::Start).unwrap();
        while *g.get_state() != State::Completed {
            match g.get_state() {
                State::Bidding(_) => {
                    g.play(GameTransition::Bet(3)).unwrap();
                }
                State::Trick(_) => {
                    let legal = g.get_legal_cards().unwrap();
                    g.play(GameTransition::Card(legal[0])).unwrap();
                }
                _ => unreachable!(),
            }
        }
        assert_eq!(*g.get_state(), State::Completed);
        // Both must be Some — the final trick's winner must be retained on GameOver.
        assert!(
            g.get_last_completed_trick().is_some(),
            "get_last_completed_trick() should be Some after game over"
        );
        assert!(
            g.get_last_trick_winner_id().is_some(),
            "get_last_trick_winner_id() should be Some after game over"
        );
    }
}
#[cfg(test)]
mod id_codec_tests {
    use super::*;

    #[test]
    fn short_id_round_trips() {
        let id = Uuid::from_u128(0xdead_beef_cafe_babe_0123_4567_89ab_cdef);
        let short = uuid_to_short_id(id);
        assert_eq!(short_id_to_uuid(&short), Some(id));
    }

    #[test]
    fn short_id_rejects_malformed() {
        // Empty input decodes to zero numbers, not two.
        assert_eq!(short_id_to_uuid(""), None);
        // A player-url token decodes to four numbers, not two.
        let token = encode_player_url(Uuid::from_u128(1), Uuid::from_u128(2));
        assert_eq!(short_id_to_uuid(&token), None);
    }

    #[test]
    fn player_url_round_trips() {
        let game = Uuid::from_u128(0x1111_2222_3333_4444_5555_6666_7777_8888);
        let player = Uuid::from_u128(0x8888_7777_6666_5555_4444_3333_2222_1111);
        let token = encode_player_url(game, player);
        assert_eq!(decode_player_url(&token), Some((game, player)));
    }

    #[test]
    fn player_url_rejects_malformed() {
        assert_eq!(decode_player_url(""), None);
        // A short id decodes to two numbers, not four.
        let short = uuid_to_short_id(Uuid::from_u128(7));
        assert_eq!(decode_player_url(&short), None);
    }
}
