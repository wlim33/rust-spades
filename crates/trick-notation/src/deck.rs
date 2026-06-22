//! A self-describing deck: declared suits and ranks (single-char symbols in
//! Phase 1). A generic parser needs no built-in deck knowledge.

use serde::{Deserialize, Serialize};

use crate::card::{Card, Sym};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(oasgen::OaSchema))]
pub struct Deck {
    pub suits: Vec<Sym>,
    pub ranks: Vec<Sym>,
}

fn chars_to_syms(s: &str) -> Vec<Sym> {
    s.chars().map(|c| c.to_string()).collect()
}

impl Deck {
    pub fn french52() -> Deck {
        Deck {
            suits: chars_to_syms("SHDC"),
            ranks: chars_to_syms("23456789TJQKA"),
        }
    }

    pub fn euchre24() -> Deck {
        Deck {
            suits: chars_to_syms("SHDC"),
            ranks: chars_to_syms("9TJQKA"),
        }
    }

    pub fn preset(name: &str) -> Option<Deck> {
        match name {
            "french52" => Some(Deck::french52()),
            "euchre24" => Some(Deck::euchre24()),
            _ => None,
        }
    }

    /// Parse a `[Deck "…"]` value: a preset name, or `suits=… ranks=…`.
    pub fn parse_decl(s: &str) -> Option<Deck> {
        let s = s.trim();
        if let Some(d) = Deck::preset(s) {
            return Some(d);
        }
        let mut suits = None;
        let mut ranks = None;
        for field in s.split_whitespace() {
            let (key, val) = field.split_once('=')?;
            match key {
                "suits" => suits = Some(chars_to_syms(val)),
                "ranks" => ranks = Some(chars_to_syms(val)),
                _ => return None,
            }
        }
        Some(Deck {
            suits: suits?,
            ranks: ranks?,
        })
    }

    /// Emit the value for a `[Deck "…"]` header: a preset name if one matches,
    /// otherwise an inline `suits=… ranks=…` declaration.
    pub fn decl_string(&self) -> String {
        for name in ["french52", "euchre24"] {
            if Deck::preset(name).as_ref() == Some(self) {
                return name.to_string();
            }
        }
        let suits: String = self.suits.concat();
        let ranks: String = self.ranks.concat();
        format!("suits={suits} ranks={ranks}")
    }

    /// Canonical card enumeration: suits in declared order, ranks within each.
    pub fn cards(&self) -> Vec<Card> {
        let mut out = Vec::with_capacity(self.suits.len() * self.ranks.len());
        for suit in &self.suits {
            for rank in &self.ranks {
                out.push(Card::Suited {
                    suit: suit.clone(),
                    rank: rank.clone(),
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french52_has_52_cards_in_canonical_order() {
        let d = Deck::french52();
        let cards = d.cards();
        assert_eq!(cards.len(), 52);
        assert_eq!(
            cards[0],
            Card::Suited {
                suit: "S".into(),
                rank: "2".into()
            }
        );
        assert_eq!(
            cards[51],
            Card::Suited {
                suit: "C".into(),
                rank: "A".into()
            }
        );
    }

    #[test]
    fn euchre24_has_24_cards() {
        assert_eq!(Deck::euchre24().cards().len(), 24);
    }

    #[test]
    fn preset_lookup_and_decl_string_round_trip() {
        let d = Deck::preset("french52").unwrap();
        assert_eq!(d, Deck::french52());
        assert_eq!(d.decl_string(), "french52");
    }

    #[test]
    fn inline_decl_parses_and_emits() {
        let d = Deck::parse_decl("suits=SHDC ranks=9TJQKA").unwrap();
        assert_eq!(d, Deck::euchre24());
        // euchre24 matches a preset, so decl_string prefers the preset name.
        assert_eq!(d.decl_string(), "euchre24");
    }

    #[test]
    fn inline_decl_emits_when_no_preset_matches() {
        let d = Deck::parse_decl("suits=AB ranks=12").unwrap();
        assert_eq!(d.decl_string(), "suits=AB ranks=12");
    }
}
