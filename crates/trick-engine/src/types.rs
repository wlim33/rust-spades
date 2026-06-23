//! Engine value types: seats, teams, bid descriptors, the per-play context the
//! ruleset reads, the per-round outcome it scores, the phase enum, and players.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use trick_notation::{Card, Deck};

/// A seat index in `0..seat_count`.
pub type Seat = usize;

/// A scoring group. Games with partnerships map several seats to one `TeamId`;
/// games without partners map every seat to its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub struct TeamId(pub usize);

/// Describes a game's bidding phase for generic readers (inclusive bounds).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BidSpec {
    pub min: i32,
    pub max: i32,
}

/// Everything a ruleset needs to decide a legal play: the actor's `hand`, the
/// cards on the `table` this trick (index = seat, `None` = not yet played), the
/// `leader` seat, and the 0-based `round`.
pub struct PlayContext<'a> {
    pub hand: &'a [Card],
    pub table: &'a [Option<Card>],
    pub leader: Seat,
    pub round: usize,
}

/// The result of a completed round, handed to `Ruleset::score_round`.
/// `tricks_won[seat]` and `bids[seat]` are seat-indexed; `bids` is all-zero when
/// the game has no bidding phase.
pub struct RoundOutcome {
    pub tricks_won: Vec<i32>,
    pub bids: Vec<i32>,
}

/// Current engine phase. The inner `usize` of `Bidding`/`Trick` is the count of
/// actors who have acted in the current rotation (matches the legacy spades
/// `State` shape so the facade can alias it).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum State {
    NotStarted,
    Bidding(usize),
    Trick(usize),
    Completed,
    Aborted,
}

#[cfg(feature = "openapi")]
impl oasgen::OaSchema for State {
    fn schema() -> oasgen::Schema {
        use oasgen::{ObjectType, Ref, Schema, SchemaData, SchemaKind, Type};

        let mut bidding = ObjectType::default();
        bidding
            .properties
            .insert("Bidding".to_string(), Schema::new_integer());
        bidding.required.push("Bidding".to_string());

        let mut trick = ObjectType::default();
        trick
            .properties
            .insert("Trick".to_string(), Schema::new_integer());
        trick.required.push("Trick".to_string());

        Schema::new_one_of(vec![
            Ref::Item(Schema::new_str_enum(vec![
                "NotStarted".to_string(),
                "Completed".to_string(),
                "Aborted".to_string(),
            ])),
            Ref::Item(Schema {
                data: SchemaData::default(),
                kind: SchemaKind::Type(Type::Object(bidding)),
            }),
            Ref::Item(Schema {
                data: SchemaData::default(),
                kind: SchemaKind::Type(Type::Object(trick)),
            }),
        ])
    }
}

/// A seated player: stable `id`, current `hand`, optional display `name`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Player {
    pub id: Uuid,
    pub hand: Vec<Card>,
    #[serde(default)]
    pub name: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_serde_round_trips() {
        for s in [
            State::NotStarted,
            State::Bidding(2),
            State::Trick(0),
            State::Completed,
            State::Aborted,
        ] {
            let j = serde_json::to_string(&s).unwrap();
            let back: State = serde_json::from_str(&j).unwrap();
            assert_eq!(s, back);
        }
    }

    #[test]
    fn player_new_starts_empty() {
        let p = Player::new(uuid::Uuid::from_u128(7));
        assert!(p.hand.is_empty());
        assert_eq!(p.name, None);
    }
}
