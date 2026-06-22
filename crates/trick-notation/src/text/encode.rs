use std::fmt::Write as _;

use crate::card::format_card;
use crate::model::{Event, Model};

use super::format_holdings;

/// Serialize a model to canonical trick-notation text. Deterministic.
pub fn to_text(model: &Model) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("% trick-notation v1\n");

    let m = &model.meta;
    if let Some(g) = &m.game_hint {
        let _ = writeln!(out, r#"[Game "{g}"]"#);
    }
    let _ = writeln!(out, r#"[Deck "{}"]"#, model.deck.decl_string());
    let _ = writeln!(out, r#"[Seats "{}"]"#, m.seats.join(" "));
    if let Some(d) = &m.dealer {
        let _ = writeln!(out, r#"[Dealer "{d}"]"#);
    }
    if m.players.iter().any(|p| p.is_some()) {
        let names: Vec<&str> = m
            .players
            .iter()
            .map(|p| p.as_deref().unwrap_or("?"))
            .collect();
        let _ = writeln!(out, r#"[Players "{}"]"#, names.join(" "));
    }
    if let Some(parts) = &m.partnerships {
        let groups: Vec<String> = parts.iter().map(|g| g.join("")).collect();
        let _ = writeln!(out, r#"[Partnerships "{}"]"#, groups.join(" "));
    }
    if !m.caps.is_empty() {
        let _ = writeln!(out, r#"[Caps "{}"]"#, m.caps.join(" "));
    }
    for (k, v) in &m.extra {
        let _ = writeln!(out, r#"[{k} "{v}"]"#);
    }
    out.push('\n');

    for event in &model.events {
        match event {
            Event::Deal { hands } => {
                out.push('D');
                for h in hands {
                    let _ = write!(out, " {}:{}", h.target, format_holdings(&h.cards, &model.deck));
                }
                out.push('\n');
            }
            Event::Call { start, values } => {
                let _ = writeln!(out, "C {start}: {}", values.join(" "));
            }
            Event::Play { leader, cards } => {
                let toks: Vec<String> = cards.iter().map(format_card).collect();
                let _ = writeln!(out, "P {leader} {}", toks.join(" "));
            }
            Event::Exchange { from, to, cards } => {
                let toks: Vec<String> = cards.iter().map(format_card).collect();
                let _ = writeln!(out, "X {from}>{to}: {}", toks.join(" "));
            }
            Event::Reveal { target, cards } => {
                let toks: Vec<String> = cards.iter().map(format_card).collect();
                let _ = writeln!(out, "U {target}:{}", toks.join(" "));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Card;
    use crate::deck::Deck;
    use crate::model::{DealtHand, Event, Meta, Model};

    fn card(rank: &str, suit: &str) -> Card {
        Card::Suited {
            suit: suit.into(),
            rank: rank.into(),
        }
    }

    #[test]
    fn encodes_headers_and_events() {
        let m = Model {
            meta: Meta {
                version: 1,
                game_hint: Some("spades".into()),
                seats: vec!["N".into(), "E".into(), "S".into(), "W".into()],
                dealer: Some("N".into()),
                players: vec![
                    Some("Ann".into()),
                    Some("Bo".into()),
                    Some("Cy".into()),
                    Some("Di".into()),
                ],
                partnerships: None,
                caps: vec![],
                extra: vec![("MaxPoints".into(), "250".into())],
            },
            deck: Deck::french52(),
            events: vec![
                Event::Deal {
                    hands: vec![DealtHand {
                        target: "N".into(),
                        cards: vec![card("A", "S"), card("K", "S"), card("T", "H")],
                    }],
                },
                Event::Call {
                    start: "E".into(),
                    values: vec!["3".into(), "4".into(), "nil".into(), "4".into()],
                },
                Event::Play {
                    leader: "E".into(),
                    cards: vec![card("K", "C"), card("5", "C")],
                },
            ],
        };
        let text = to_text(&m);
        assert!(text.starts_with("% trick-notation v1\n"), "got:\n{text}");
        assert!(text.contains(r#"[Game "spades"]"#), "{text}");
        assert!(text.contains(r#"[Deck "french52"]"#), "{text}");
        assert!(text.contains(r#"[Seats "N E S W"]"#), "{text}");
        assert!(text.contains(r#"[MaxPoints "250"]"#), "{text}");
        // dot-grouped holding: spades AK, hearts T, diamonds void, clubs void
        assert!(text.contains("D N:AK.T.-.-"), "{text}");
        assert!(text.contains("C E: 3 4 nil 4"), "{text}");
        assert!(text.contains("P E KC 5C"), "{text}");
    }
}
