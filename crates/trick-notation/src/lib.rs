//! Game-agnostic notation for trick-taking card games.
//!
//! One in-memory [`Model`] serializes to canonical text and JSON. The model is
//! rule-agnostic: it records observed events (deals, calls, plays, exchanges),
//! never rule-derived facts like trick winners or scores. The leader seat of
//! every trick is recorded explicitly so a generic reader can lay out a game
//! without knowing any rules.

mod card;
pub use card::{Card, Sym, format_card, parse_card};

mod deck;
pub use deck::Deck;
