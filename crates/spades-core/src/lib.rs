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

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Player {
    id: Uuid,
    hand: Vec<Card>,
    #[serde(default)]
    name: Option<String>,
}

impl Player {
    pub fn new(id: Uuid) -> Player {
        Player {
            id,
            hand: vec![],
            name: None,
        }
    }
}

/// Primary game state. Internally manages player rotation, scoring, and cards.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Game {
    id: Uuid,
    state: State,
    scoring: scoring::Scoring,
    current_player_index: usize,
    deck: Vec<cards::Card>,
    hands_played: Vec<[Option<cards::Card>; 4]>,
    leading_suit: Option<Suit>,
    players: [Player; 4],
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
    #[serde(default)]
    spades_broken: bool,
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
        Game {
            id,
            state: State::NotStarted,
            scoring: scoring::Scoring::new(max_points),
            hands_played: vec![[None; 4]],
            deck: cards::new_deck(),
            current_player_index: 0,
            leading_suit: None,
            players: [
                Player::new(player_ids[0]),
                Player::new(player_ids[1]),
                Player::new(player_ids[2]),
                Player::new(player_ids[3]),
            ],
            timer_config,
            player_clocks,
            turn_started_at_epoch_ms: None,
            last_trick_winner: None,
            last_completed_trick: None,
            spades_broken: false,
        }
    }

    /// The game's unique id.
    pub fn get_id(&self) -> &Uuid {
        &self.id
    }

    /// See [`State`](enum.State.html)
    pub fn get_state(&self) -> &State {
        &self.state
    }

    /// `Ok` once the game has started — i.e. any state other than [`State::NotStarted`].
    fn require_started(&self) -> Result<(), GetError> {
        match self.state {
            State::NotStarted => Err(GetError::GameNotStarted),
            _ => Ok(()),
        }
    }

    /// `Ok` only while a hand is in progress (Betting or Trick), distinguishing
    /// not-yet-started from already-finished.
    fn require_active(&self) -> Result<(), GetError> {
        match self.state {
            State::NotStarted => Err(GetError::GameNotStarted),
            State::Completed | State::Aborted => Err(GetError::GameCompleted),
            State::Betting(_) | State::Trick(_) => Ok(()),
        }
    }

    /// Team A's (seats 0 & 2) cumulative score. `Err` before the game has started.
    pub fn get_team_a_score(&self) -> Result<i32, GetError> {
        self.require_started()?;
        Ok(self.scoring.team_a.cumulative_points)
    }

    /// Team B's (seats 1 & 3) cumulative score. `Err` before the game has started.
    pub fn get_team_b_score(&self) -> Result<i32, GetError> {
        self.require_started()?;
        Ok(self.scoring.team_b.cumulative_points)
    }

    /// Team A's accumulated bags (overtricks). `Err` before the game has started.
    pub fn get_team_a_bags(&self) -> Result<i32, GetError> {
        self.require_started()?;
        Ok(self.scoring.team_a.bags)
    }

    /// Team B's accumulated bags (overtricks). `Err` before the game has started.
    pub fn get_team_b_bags(&self) -> Result<i32, GetError> {
        self.require_started()?;
        Ok(self.scoring.team_b.bags)
    }

    /// Returns `GetError` when the current game is not in the Betting or Trick stages.
    pub fn get_current_player_id(&self) -> Result<Uuid, GetError> {
        self.require_active()?;
        Ok(self.players[self.current_player_index].id)
    }

    /// Returns a `GetError::InvalidUuid` if the game does not contain a player with the given `Uuid`.
    pub fn get_hand_by_player_id(&self, player_id: Uuid) -> Result<&Vec<Card>, GetError> {
        self.players
            .iter()
            .find(|p| p.id == player_id)
            .map(|p| &p.hand)
            .ok_or(GetError::InvalidUuid)
    }

    /// The hand of the player whose turn it is. `Err` unless the game is in the Betting or Trick stage.
    pub fn get_current_hand(&self) -> Result<&Vec<Card>, GetError> {
        self.require_active()?;
        Ok(&self.players[self.current_player_index].hand)
    }

    /// The suit led in the current trick, or `None` if no card has been led yet. Only valid in the Trick stage.
    pub fn get_leading_suit(&self) -> Result<Option<Suit>, GetError> {
        match &self.state {
            State::NotStarted => Err(GetError::GameNotStarted),
            State::Completed => Err(GetError::GameCompleted),
            State::Trick(_) => Ok(self.leading_suit),
            _ => Err(GetError::Unknown),
        }
    }

    /// Returns the cards currently on the table; each slot is `None` if that player
    /// hasn't yet played this trick. Only available in the Trick stage.
    pub fn get_current_trick_cards(&self) -> Result<&[Option<cards::Card>; 4], GetError> {
        match self.state {
            State::NotStarted => Err(GetError::GameNotStarted),
            State::Completed | State::Aborted => Err(GetError::GameCompleted),
            State::Betting(_) => Err(GetError::Unknown),
            State::Trick(_) => Ok(self.hands_played.last().unwrap()),
        }
    }

    /// The ids of the two players on the winning team. `Err` unless the game has completed.
    pub fn get_winner_ids(&self) -> Result<(Uuid, Uuid), GetError> {
        match self.state {
            State::Completed => {
                if self.scoring.team_a.cumulative_points > self.scoring.team_b.cumulative_points {
                    Ok((self.players[0].id, self.players[2].id))
                } else if self.scoring.team_b.cumulative_points
                    > self.scoring.team_a.cumulative_points
                {
                    Ok((self.players[1].id, self.players[3].id))
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
        match entry {
            GameTransition::Bet(bet) => match self.state {
                State::NotStarted => Err(TransitionError::NotStarted),
                State::Trick(_rotation_status) => Err(TransitionError::BetInTrickStage),
                State::Completed | State::Aborted => Err(TransitionError::CompletedGame),
                State::Betting(rotation_status) => {
                    if !(0..=13).contains(&bet) {
                        return Err(TransitionError::InvalidBet);
                    }
                    self.scoring.add_bet(self.current_player_index, bet);
                    if rotation_status == 3 {
                        self.scoring.bet();
                        self.state = State::Trick((rotation_status + 1) % 4);
                        self.current_player_index = 0;
                        return Ok(TransitionSuccess::BetComplete);
                    } else {
                        self.current_player_index = (self.current_player_index + 1) % 4;
                        self.state = State::Betting((rotation_status + 1) % 4);
                    }

                    Ok(TransitionSuccess::Bet)
                }
            },
            GameTransition::Card(card) => {
                match self.state {
                    State::NotStarted => Err(TransitionError::NotStarted),
                    State::Completed | State::Aborted => Err(TransitionError::CompletedGame),
                    State::Betting(_rotation_status) => Err(TransitionError::CardInBettingStage),
                    State::Trick(rotation_status) => {
                        {
                            let player_hand = &mut self.players[self.current_player_index].hand;

                            if !player_hand.contains(&card) {
                                return Err(TransitionError::CardNotInHand);
                            }
                            if rotation_status == 0
                                && card.suit == Suit::Spade
                                && !self.spades_broken
                                && player_hand.iter().any(|c| c.suit != Suit::Spade)
                            {
                                return Err(TransitionError::SpadesNotBroken);
                            }
                            if rotation_status == 0 {
                                self.leading_suit = Some(card.suit);
                            } else if let Some(ls) = self.leading_suit
                                && ls != card.suit
                                && player_hand.iter().any(|x| x.suit == ls)
                            {
                                return Err(TransitionError::CardIncorrectSuit);
                            }

                            let card_index = player_hand.iter().position(|x| x == &card).unwrap();
                            self.deck.push(player_hand.remove(card_index));
                        }

                        if card.suit == Suit::Spade {
                            self.spades_broken = true;
                        }
                        self.hands_played.last_mut().unwrap()[self.current_player_index] =
                            Some(card);

                        if rotation_status == 3 {
                            let trick = self.hands_played.last().unwrap();
                            let played: [Card; 4] = [
                                trick[0].unwrap(),
                                trick[1].unwrap(),
                                trick[2].unwrap(),
                                trick[3].unwrap(),
                            ];
                            // Trick complete: current_player_index is the LAST seat that played;
                            // the lead seat is the next-in-rotation (winner of the trick is computed
                            // from the lead's perspective, since the lead's card sets the trick suit).
                            let lead = (self.current_player_index + 1) % 4;
                            let winner = self.scoring.trick(lead, &played);
                            self.last_trick_winner = Some(winner);
                            self.last_completed_trick = Some(played);
                            if self.scoring.is_over {
                                self.state = State::Completed;
                                return Ok(TransitionSuccess::GameOver);
                            }
                            if self.scoring.in_betting_stage {
                                self.last_trick_winner = None;
                                self.current_player_index = 0;
                                self.state = State::Betting((rotation_status + 1) % 4);
                                self.spades_broken = false;
                                self.leading_suit = None;
                                self.deal_cards();
                                self.hands_played.push([None; 4]);
                            } else {
                                self.current_player_index = winner;
                                self.state = State::Trick((rotation_status + 1) % 4);
                                self.leading_suit = None;
                                self.hands_played.push([None; 4]);
                            }
                            Ok(TransitionSuccess::Trick)
                        } else {
                            self.current_player_index = (self.current_player_index + 1) % 4;
                            self.state = State::Trick((rotation_status + 1) % 4);
                            Ok(TransitionSuccess::PlayCard)
                        }
                    }
                }
            }
            GameTransition::Start => {
                if self.state != State::NotStarted {
                    return Err(TransitionError::AlreadyStarted);
                }
                self.deal_cards();
                self.state = State::Betting(0);
                Ok(TransitionSuccess::Start)
            }
            GameTransition::Abort => match self.state {
                State::Completed | State::Aborted => Err(TransitionError::CompletedGame),
                _ => {
                    self.state = State::Aborted;
                    Ok(TransitionSuccess::Aborted)
                }
            },
        }
    }

    /// Set (or clear, with `None`) a player's display name. `Err` if no player matches `player_id`.
    pub fn set_player_name(
        &mut self,
        player_id: Uuid,
        name: Option<String>,
    ) -> Result<(), GetError> {
        let p = self
            .players
            .iter_mut()
            .find(|p| p.id == player_id)
            .ok_or(GetError::InvalidUuid)?;
        p.name = name;
        Ok(())
    }

    /// Each seat's `(id, display name)` in seat order.
    pub fn get_player_names(&self) -> [(Uuid, Option<&str>); 4] {
        std::array::from_fn(|i| (self.players[i].id, self.players[i].name.as_deref()))
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
        self.current_player_index
    }

    /// Returns true if the game is in the first round's betting phase (round 0, Betting state).
    pub fn is_first_round_betting(&self) -> bool {
        self.scoring.round == 0 && matches!(self.state, State::Betting(_))
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
        match self.state {
            State::NotStarted => None,
            _ => Some(self.scoring.bets_placed[self.scoring.round]),
        }
    }

    /// Each seat's tricks won so far this round, or `None` before the game has started.
    pub fn get_player_tricks_won(&self) -> Option<[i32; 4]> {
        match self.state {
            State::NotStarted => None,
            _ => Some(self.scoring.player_tricks_won),
        }
    }

    /// The id of the player who won the most recently completed trick, or `None`
    /// between rounds and before any trick completes.
    pub fn get_last_trick_winner_id(&self) -> Option<Uuid> {
        // `last_trick_winner` is always a valid seat by construction; `.min(3)`
        // defensively guards against an out-of-range index from a corrupt
        // deserialized row (prefer a wrong-but-safe id over a panic here).
        self.last_trick_winner
            .map(|idx| self.players[idx.min(3)].id)
    }

    /// The four cards of the most recently completed trick, or `None` if none has completed this round.
    pub fn get_last_completed_trick(&self) -> Option<&[cards::Card; 4]> {
        self.last_completed_trick.as_ref()
    }

    /// Set the game state directly. Crate-internal escape hatch for transcript
    /// replay and tests; external callers abort via [`GameTransition::Abort`].
    pub(crate) fn set_state(&mut self, state: State) {
        self.state = state;
    }

    /// Returns the list of legal cards the current player can play.
    /// Only valid in the Trick state.
    pub fn get_legal_cards(&self) -> Result<Vec<Card>, GetError> {
        match &self.state {
            State::Trick(rotation_status) => {
                let hand = self.get_current_hand()?;
                if *rotation_status == 0 {
                    if !self.spades_broken {
                        let non_spades: Vec<Card> = hand
                            .iter()
                            .filter(|c| c.suit != Suit::Spade)
                            .copied()
                            .collect();
                        if !non_spades.is_empty() {
                            return Ok(non_spades);
                        }
                    }
                    Ok(hand.clone())
                } else if let Some(ls) = self.leading_suit {
                    let has_leading_suit = hand.iter().any(|c| c.suit == ls);
                    if has_leading_suit {
                        Ok(hand.iter().filter(|c| c.suit == ls).copied().collect())
                    } else {
                        Ok(hand.clone())
                    }
                } else {
                    Ok(hand.clone())
                }
            }
            _ => Err(GetError::Unknown),
        }
    }

    /// Max points configured at game creation.
    pub fn get_max_points(&self) -> i32 {
        self.scoring.config.max_points
    }

    /// All trick slots, one per trick. For round R the slots live at indices
    /// 13*R .. 13*(R+1). The final slot may be partially filled (current trick).
    /// Empty trailing slot during betting between rounds is intentional.
    pub fn get_history(&self) -> &[[Option<cards::Card>; 4]] {
        &self.hands_played
    }

    /// All bets per round in seat order. `bets_placed[R][s]` is seat `s`'s bet
    /// for round `R`. The trailing entry is a write target for the next round's
    /// bets and may be all zeros even when no bets have been placed.
    pub fn get_all_bets(&self) -> &[[i32; 4]] {
        &self.scoring.bets_placed
    }

    /// Current 0-based round index (`scoring.round`).
    pub fn get_round_index(&self) -> usize {
        self.scoring.round
    }

    /// True when the game is in (or just finished) a betting phase rather than
    /// a trick phase. Combined with `get_state()` this disambiguates Aborted
    /// games.
    pub fn is_in_betting_stage(&self) -> bool {
        self.scoring.in_betting_stage
    }

    fn deal_cards(&mut self) {
        cards::shuffle(&mut self.deck);
        let mut hands = cards::deal_four_players(&mut self.deck);
        for i in (0..4).rev() {
            self.players[i].hand = hands.pop().unwrap();
            self.players[i].hand.sort();
        }
    }

    /// Override each player's hand with the given cards (used by transcript replay
    /// to seed the engine with the hands declared in the transcript rather than the
    /// randomly-dealt ones).  The caller is responsible for correctness; this does
    /// not validate that the supplied cards form a legal deal.
    pub(crate) fn override_hands(&mut self, hands: [Vec<Card>; 4]) {
        for (i, hand) in hands.into_iter().enumerate() {
            self.players[i].hand = hand;
        }
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
