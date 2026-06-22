//! The pure, rule-agnostic model: metadata, a self-describing deck, and an
//! ordered event stream. A deal is an event (games re-deal every round).

use serde::{Deserialize, Serialize};

use crate::card::{Card, Sym};
use crate::deck::Deck;

/// A deal target: a seat symbol, or a named pile written `@kitty`.
pub type Target = String;

/// One hand in a `Deal` event: the target seat/pile and the cards dealt to it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(oasgen::OaSchema))]
pub struct DealtHand {
    pub target: Target,
    pub cards: Vec<Card>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(oasgen::OaSchema))]
pub struct Meta {
    pub version: u8,
    pub game_hint: Option<Sym>,
    pub seats: Vec<Sym>,
    pub dealer: Option<Sym>,
    pub players: Vec<Option<String>>,
    pub partnerships: Option<Vec<Vec<Sym>>>,
    pub caps: Vec<Sym>,
    /// Open tag namespace for game-specific config (e.g. spades MaxPoints).
    /// Serialized as `[["key", "value"], …]`; omitted from the OpenAPI schema
    /// because oasgen 0.25 does not implement OaSchema for tuple types.
    #[cfg_attr(feature = "openapi", oasgen(skip))]
    pub extra: Vec<(String, String)>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(oasgen::OaSchema))]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Event {
    Deal {
        hands: Vec<DealtHand>,
    },
    Call {
        start: Sym,
        values: Vec<Sym>,
    },
    Play {
        leader: Sym,
        cards: Vec<Card>,
    },
    Exchange {
        from: Sym,
        to: Sym,
        cards: Vec<Card>,
    },
    Reveal {
        target: Target,
        cards: Vec<Card>,
    },
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(oasgen::OaSchema))]
pub struct Model {
    pub meta: Meta,
    pub deck: Deck,
    pub events: Vec<Event>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Card;

    fn sample() -> Model {
        Model {
            meta: Meta {
                version: 1,
                game_hint: Some("spades".into()),
                seats: vec!["N".into(), "E".into(), "S".into(), "W".into()],
                dealer: Some("N".into()),
                players: vec![Some("Ann".into()), None, None, None],
                partnerships: None,
                caps: vec![],
                extra: vec![("MaxPoints".into(), "250".into())],
            },
            deck: Deck::french52(),
            events: vec![
                Event::Call {
                    start: "E".into(),
                    values: vec!["3".into(), "4".into(), "nil".into(), "4".into()],
                },
                Event::Play {
                    leader: "E".into(),
                    cards: vec![
                        Card::Suited {
                            suit: "C".into(),
                            rank: "K".into(),
                        },
                        Card::Suited {
                            suit: "C".into(),
                            rank: "5".into(),
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn model_json_round_trips() {
        let m = sample();
        let json = serde_json::to_string(&m).unwrap();
        let back: Model = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }
}
