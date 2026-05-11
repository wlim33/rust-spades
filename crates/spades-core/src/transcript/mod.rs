//! Spades Transcript Format (STF) — PGN-inspired serialization of full game history.
//!
//! See `docs/superpowers/specs/2026-05-11-spades-transcript-format-design.md`.

use std::fmt;
use uuid::Uuid;

use crate::cards::Card;
use crate::result::TransitionError;
use crate::TimerConfig;

mod format;
mod encode;
mod decode;
mod replay;

pub use encode::encode;
pub use decode::decode;
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
    DuplicateRound { round: usize },
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
    /// One of TimerInitial/TimerIncrement was present without the other.
    TimerHalfSpecified,
    /// Round R seat S's declared dealt hand contradicts the cards actually played.
    HandMismatch { round: usize, seat: usize },
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
