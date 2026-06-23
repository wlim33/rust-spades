//! Spades surfaces the engine's `State`, but keeps the historical `Betting`
//! name (the engine calls the same phase `Bidding`). Both name the same value.

pub use trick_engine::State;

/// Back-compat constructor for the bidding phase, named `Betting` in spades.
/// The engine's variant is `State::Bidding(rotation)`; match on that.
#[allow(dead_code)]
pub fn betting(rotation: usize) -> State {
    State::Bidding(rotation)
}
