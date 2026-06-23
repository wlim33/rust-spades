//! The pluggable rule surface. The engine owns the round/trick/rotation
//! skeleton; everything game-specific is one of these twelve questions. The
//! trait is object-safe and `#[typetag::serde]`-tagged so `Box<dyn Ruleset>`
//! serializes (and rebuilds) with a `"type"` discriminant.

use crate::types::{BidSpec, Card, PlayContext, RoundOutcome, Seat, TeamId};

#[typetag::serde(tag = "type")]
pub trait Ruleset {
    /// Number of seats at the table.
    fn seat_count(&self) -> usize;
    /// The scoring group a seat belongs to.
    fn team_of(&self, seat: Seat) -> TeamId;

    /// The full deck for a deal, in any order (the engine shuffles).
    fn build_deck(&self) -> Vec<Card>;
    /// Cards dealt to each seat for `round`.
    fn hand_size(&self, round: usize) -> usize;
    /// The seat that leads the first trick of `round`.
    fn first_leader(&self, round: usize) -> Seat;

    /// `Some(spec)` if `round` opens with a bidding phase, else `None`.
    fn bid_phase(&self) -> Option<BidSpec>;
    /// Whether `bid` from `seat` is legal.
    fn bid_is_legal(&self, seat: Seat, bid: i32) -> bool;

    /// The subset of the actor's hand that is legal to play now.
    fn legal_plays(&self, ctx: &PlayContext) -> Vec<Card>;
    /// The winning seat of a completed trick, given the `leader` and the cards
    /// each seat played (index = seat).
    fn trick_winner(&self, leader: Seat, played: &[Card]) -> Seat;

    /// Fold a completed round's outcome into the ruleset's own score state.
    fn score_round(&mut self, outcome: &RoundOutcome);
    /// Whether the game has reached a terminal score.
    fn is_over(&self) -> bool;
    /// Current cumulative score per `TeamId` index, for generic readers.
    fn scores(&self) -> Vec<i32>;
}
