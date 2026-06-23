//! Game-agnostic trick-taking card-game engine. A generic [`Game`] state machine
//! drives deal/bid/trick/score rounds, deferring every rule-specific decision to
//! a [`Ruleset`] trait object. Card identity is reused from `trick_notation`.

mod ruleset;
mod types;
mod game;

pub use ruleset::Ruleset;
pub use game::{Action, Game, StepError, StepOutcome};
pub use types::*;

#[cfg(test)]
mod testkit;
