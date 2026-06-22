//! A card is identity-only: a suited card (rank within a suit) or a named
//! special (joker, the Fool). No rank ordering is implied — ordering is a
//! per-game rule, not a property of the card.

use serde::{Deserialize, Serialize};

/// A short symbol for a suit, rank, seat, or special. Usually one character.
pub type Sym = String;

#[derive(Clone, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Card {
    Suited { suit: Sym, rank: Sym },
    Special { name: Sym },
}

/// Render a card as a canonical token: `rank` then `suit` for suited cards
/// (`KC`), or `*name` for specials (`*Fool`).
pub fn format_card(c: &Card) -> String {
    match c {
        Card::Suited { suit, rank } => format!("{rank}{suit}"),
        Card::Special { name } => format!("*{name}"),
    }
}

/// Parse a canonical card token. Phase 1 accepts single-character rank+suit
/// (`KC`) and `*name` specials. Returns `None` on anything else.
pub fn parse_card(tok: &str) -> Option<Card> {
    if let Some(name) = tok.strip_prefix('*') {
        if name.is_empty() {
            return None;
        }
        return Some(Card::Special {
            name: name.to_string(),
        });
    }
    let mut chars = tok.chars();
    let rank = chars.next()?;
    let suit = chars.next()?;
    if chars.next().is_some() {
        return None; // more than two chars and not a special
    }
    Some(Card::Suited {
        suit: suit.to_string(),
        rank: rank.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suited_card_round_trips() {
        let c = Card::Suited {
            suit: "C".into(),
            rank: "K".into(),
        };
        assert_eq!(format_card(&c), "KC");
        assert_eq!(parse_card("KC"), Some(c));
    }

    #[test]
    fn special_card_round_trips() {
        let c = Card::Special {
            name: "Fool".into(),
        };
        assert_eq!(format_card(&c), "*Fool");
        assert_eq!(parse_card("*Fool"), Some(c));
    }

    #[test]
    fn parse_rejects_bad_tokens() {
        assert_eq!(parse_card(""), None);
        assert_eq!(parse_card("K"), None);
        assert_eq!(parse_card("*"), None);
    }
}
