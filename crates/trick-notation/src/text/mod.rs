mod decode;
mod encode;

pub use decode::{ParseError, from_text};
pub use encode::to_text;

use crate::card::Card;
use crate::deck::Deck;

/// Dot-grouped holdings (PBN-style): one group per deck suit in declared order,
/// ranks concatenated within a group; a void suit is written `-`.
pub(crate) fn format_holdings(cards: &[Card], deck: &Deck) -> String {
    let mut groups: Vec<String> = Vec::with_capacity(deck.suits.len());
    for suit in &deck.suits {
        let mut ranks = String::new();
        // Emit ranks in reverse deck order (high-to-low) for stable output.
        for rank in deck.ranks.iter().rev() {
            let present = cards
                .iter()
                .any(|c| matches!(c, Card::Suited { suit: s, rank: r } if s == suit && r == rank));
            if present {
                ranks.push_str(rank);
            }
        }
        groups.push(if ranks.is_empty() {
            "-".to_string()
        } else {
            ranks
        });
    }
    groups.join(".")
}

/// Inverse of [`format_holdings`]: `AK.T.-.-` → the cards, using `deck.suits`
/// order to assign each dot-group its suit.
pub(crate) fn parse_holdings(s: &str, deck: &Deck) -> Option<Vec<Card>> {
    let groups: Vec<&str> = s.split('.').collect();
    if groups.len() != deck.suits.len() {
        return None;
    }
    let mut cards = Vec::new();
    for (suit, group) in deck.suits.iter().zip(groups) {
        if group == "-" {
            continue;
        }
        for ch in group.chars() {
            cards.push(Card::Suited {
                suit: suit.clone(),
                rank: ch.to_string(),
            });
        }
    }
    Some(cards)
}
