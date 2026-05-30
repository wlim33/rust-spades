//! Spades Transcript Format (STF) — PGN-inspired serialization of full game history.
//!
//! See `docs/superpowers/specs/2026-05-11-spades-transcript-format-design.md`.
//!
//! # Known limitations
//!
//! - **Aborted-mid-betting is lossy.** When a game is aborted while in the
//!   betting phase, the encoder emits all 4 bet slots (un-placed bets default
//!   to 0). Replay then treats them as 4 real bets, so the replayed game's
//!   state will not be observationally equal to the source for this specific
//!   case. Aborted from `Trick(_)` or terminal states round-trips cleanly.

use std::fmt;
use uuid::Uuid;

use crate::TimerConfig;
use crate::cards::Card;
use crate::result::TransitionError;

mod decode;
mod encode;
mod format;
mod replay;

pub use decode::decode;
pub use encode::encode;
pub use replay::replay;

/// Parsed transcript. Constructed by `decode`, consumed by `replay`, produced
/// alongside `encode`'s String for round-trip testing helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    pub headers: Headers,
    pub rounds: Vec<Round>,
    pub termination: Termination,
    /// Final cumulative team scores, `(team_a, team_b)`. `None` when `termination == InProgress`.
    pub result: Option<(i32, i32)>,
}

/// Decoded header block: game identity, player roster, and optional timer config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Headers {
    pub game_id: Uuid,
    pub max_points: i32,
    pub player_ids: [Uuid; 4],
    pub names: [Option<String>; 4],
    pub timer: Option<TimerConfig>,
}

/// Per-round data: dealt hands per seat, bets in seat order, tricks in play order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Round {
    /// Dealt hand per seat at the start of the round, sorted by `Card::Ord`.
    pub hands: [Vec<Card>; 4],
    /// Bets in seat order. Length 0..=4; a partial vec means the round was
    /// captured mid-betting.
    pub bets: Vec<i32>,
    /// Tricks in play order. Each inner Vec has 1..=4 cards; the last trick
    /// may be partial (mid-trick capture).
    pub tricks: Vec<Vec<Card>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termination {
    Completed,
    Aborted,
    InProgress,
}

impl fmt::Display for Termination {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Termination::Completed => "Completed",
            Termination::Aborted => "Aborted",
            Termination::InProgress => "InProgress",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    UnexpectedEof,
    BadTag { line: usize, found: String },
    DuplicateTag { line: usize, key: String },
    MissingRequiredTag { key: &'static str },
    BadCard { line: usize, token: String },
    NonMonotonicRound { expected: usize, found: usize },
    TooManyTricks { round: usize },
    TooManyBets { round: usize },
    TooManyCardsInTrick { round: usize, trick: usize },
    BadResult { line: usize, value: String },
    BadTermination { line: usize, value: String },
    BadUuid { line: usize, value: String },
    BadInteger { line: usize, value: String },
    BadEscape { line: usize, value: String },
    TrailingContent { line: usize },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "transcript decode error: {:?}", self)
    }
}

impl std::error::Error for DecodeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    /// `Game::play` rejected a transition synthesized from the transcript.
    Transition {
        round: usize,
        trick: Option<usize>,
        seat: usize,
        err: TransitionError,
    },
    /// Header `Termination` doesn't match the state the replayed game ended in.
    TerminationMismatch {
        declared: Termination,
        actual: Termination,
    },
    /// Header `Result` doesn't match replayed cumulative scores.
    ResultMismatch {
        declared: (i32, i32),
        actual: (i32, i32),
    },
    /// `Bets` line had a count not matching the state when termination is final
    /// (e.g. Completed transcript with < 4 bets in a round).
    InconsistentBetCount { round: usize, found: usize },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "transcript replay error: {:?}", self)
    }
}

impl std::error::Error for ReplayError {}

#[cfg(test)]
mod display_tests {
    use super::*;
    use crate::result::TransitionError;

    #[test]
    fn termination_display_emits_canonical_words() {
        assert_eq!(Termination::Completed.to_string(), "Completed");
        assert_eq!(Termination::Aborted.to_string(), "Aborted");
        assert_eq!(Termination::InProgress.to_string(), "InProgress");
    }

    #[test]
    fn decode_error_display_prefix() {
        let err = DecodeError::UnexpectedEof;
        assert!(err.to_string().starts_with("transcript decode error:"));
    }

    #[test]
    fn replay_error_display_prefix() {
        let err = ReplayError::Transition {
            round: 0,
            trick: None,
            seat: 0,
            err: TransitionError::NotStarted,
        };
        assert!(err.to_string().starts_with("transcript replay error:"));
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::{Game, GameTransition, State};
    use rand::rngs::StdRng;
    use rand::seq::SliceRandom;
    use rand::{RngCore, SeedableRng};
    use uuid::Uuid;

    fn play_full_random_game(seed: u64) -> Game {
        let mut rng = StdRng::seed_from_u64(seed);

        let mut id_bytes = [0u8; 16];
        id_bytes[..8].copy_from_slice(&seed.to_be_bytes());
        id_bytes[8..].copy_from_slice(&(!seed).to_be_bytes());
        let game_id = Uuid::from_bytes(id_bytes);

        let player_ids = [
            Uuid::from_bytes([1; 16]),
            Uuid::from_bytes([2; 16]),
            Uuid::from_bytes([3; 16]),
            Uuid::from_bytes([4; 16]),
        ];

        let mut g = Game::new(game_id, player_ids, 60, None);
        g.play(GameTransition::Start).unwrap();
        loop {
            match *g.get_state() {
                State::Completed | State::Aborted => return g,
                State::Betting(_) => {
                    let b = (rng.next_u32() % 4) as i32 + 1;
                    g.play(GameTransition::Bet(b)).unwrap();
                }
                State::Trick(_) => {
                    let legal = g.get_legal_cards().unwrap();
                    let card = *legal.choose(&mut rng).unwrap();
                    g.play(GameTransition::Card(card)).unwrap();
                }
                State::NotStarted => unreachable!(),
            }
        }
    }

    #[test]
    fn round_trip_is_idempotent_on_many_random_games() {
        for seed in 0..30u64 {
            let g = play_full_random_game(seed);
            let s1 = encode(&g);
            let parsed = decode(&s1).expect("decode");
            let replayed = replay(&parsed).expect("replay");
            let s2 = encode(&replayed);
            assert_eq!(s1, s2, "round trip differed for seed {}", seed);
        }
    }
}
