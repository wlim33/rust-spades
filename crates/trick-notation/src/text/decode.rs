use crate::card::{Card, parse_card};
use crate::deck::Deck;
use crate::model::{Event, Meta, Model};

use super::parse_holdings;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("missing version marker '% trick-notation v1' on line 1")]
    MissingVersion,
    #[error("malformed header on line {line}: {text:?}")]
    BadHeader { line: usize, text: String },
    #[error("missing required header {key:?}")]
    MissingHeader { key: &'static str },
    #[error("unknown deck declaration {decl:?} on line {line}")]
    BadDeck { line: usize, decl: String },
    #[error("malformed event on line {line}: {text:?}")]
    BadEvent { line: usize, text: String },
    #[error("invalid card token {token:?} on line {line}")]
    BadCard { line: usize, token: String },
    #[error("invalid holdings {holding:?} on line {line}")]
    BadHoldings { line: usize, holding: String },
}

/// Parse a header line `[Key "value"]`. Returns `(key, value)`.
fn parse_header(line: &str) -> Option<(String, String)> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let (key, rest) = inner.split_once(' ')?;
    let value = rest.strip_prefix('"')?.strip_suffix('"')?;
    Some((key.to_string(), value.to_string()))
}


pub fn from_text(text: &str) -> Result<Model, ParseError> {
    let mut lines = text.lines().enumerate();

    // Line 1: version marker.
    match lines.next() {
        Some((_, l)) if l.trim() == "% trick-notation v1" => {}
        _ => return Err(ParseError::MissingVersion),
    }

    let mut meta = Meta {
        version: 1,
        game_hint: None,
        seats: vec![],
        dealer: None,
        players: vec![],
        partnerships: None,
        caps: vec![],
        extra: vec![],
    };
    let mut deck: Option<Deck> = None;
    let mut events: Vec<Event> = Vec::new();

    for (idx, raw) in lines {
        let line_no = idx + 1;
        let line = match raw.split_once(';') {
            Some((code, _comment)) => code.trim_end(),
            None => raw,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') {
            let (key, value) = parse_header(line)
                .ok_or_else(|| ParseError::BadHeader { line: line_no, text: line.to_string() })?;
            match key.as_str() {
                "Game" => meta.game_hint = Some(value),
                "Deck" => {
                    deck = Some(Deck::parse_decl(&value).ok_or(ParseError::BadDeck {
                        line: line_no,
                        decl: value.clone(),
                    })?);
                }
                "Seats" => meta.seats = value.split_whitespace().map(String::from).collect(),
                "Dealer" => meta.dealer = Some(value),
                "Players" => {
                    meta.players = value
                        .split_whitespace()
                        .map(|n| if n == "?" { None } else { Some(n.to_string()) })
                        .collect();
                }
                "Partnerships" => {
                    meta.partnerships = Some(
                        value
                            .split_whitespace()
                            .map(|g| g.chars().map(|c| c.to_string()).collect())
                            .collect(),
                    );
                }
                "Caps" => meta.caps = value.split_whitespace().map(String::from).collect(),
                _ => meta.extra.push((key, value)),
            }
            continue;
        }

        // Event lines. Deck must be known by now (events reference it).
        let deck_ref = deck
            .as_ref()
            .ok_or(ParseError::MissingHeader { key: "Deck" })?;
        events.push(parse_event(line, line_no, deck_ref)?);
    }

    if meta.seats.is_empty() {
        return Err(ParseError::MissingHeader { key: "Seats" });
    }
    let deck = deck.ok_or(ParseError::MissingHeader { key: "Deck" })?;
    Ok(Model { meta, deck, events })
}

fn cards_from_tokens(toks: &[&str], line_no: usize) -> Result<Vec<Card>, ParseError> {
    toks.iter()
        .map(|t| {
            parse_card(t).ok_or(ParseError::BadCard { line: line_no, token: t.to_string() })
        })
        .collect()
}

fn parse_event(line: &str, line_no: usize, deck: &Deck) -> Result<Event, ParseError> {
    let bad = || ParseError::BadEvent { line: line_no, text: line.to_string() };
    let (code, rest) = line.split_once(char::is_whitespace).ok_or_else(bad)?;
    let rest = rest.trim();
    match code {
        "D" => {
            let mut hands = Vec::new();
            for spec in rest.split_whitespace() {
                let (target, holding) = spec.split_once(':').ok_or_else(bad)?;
                let cards = parse_holdings(holding, deck).ok_or(ParseError::BadHoldings {
                    line: line_no,
                    holding: holding.to_string(),
                })?;
                hands.push((target.to_string(), cards));
            }
            Ok(Event::Deal { hands })
        }
        "C" => {
            let (start, vals) = rest.split_once(':').ok_or_else(bad)?;
            let values = vals.split_whitespace().map(String::from).collect();
            Ok(Event::Call { start: start.trim().to_string(), values })
        }
        "P" => {
            let (leader, cards) = rest.split_once(char::is_whitespace).ok_or_else(bad)?;
            let toks: Vec<&str> = cards.split_whitespace().collect();
            Ok(Event::Play {
                leader: leader.to_string(),
                cards: cards_from_tokens(&toks, line_no)?,
            })
        }
        "X" => {
            let (dirs, cards) = rest.split_once(':').ok_or_else(bad)?;
            let (from, to) = dirs.trim().split_once('>').ok_or_else(bad)?;
            let toks: Vec<&str> = cards.split_whitespace().collect();
            Ok(Event::Exchange {
                from: from.to_string(),
                to: to.to_string(),
                cards: cards_from_tokens(&toks, line_no)?,
            })
        }
        "U" => {
            let (target, cards) = rest.split_once(':').ok_or_else(bad)?;
            let toks: Vec<&str> = cards.split_whitespace().collect();
            Ok(Event::Reveal {
                target: target.trim().to_string(),
                cards: cards_from_tokens(&toks, line_no)?,
            })
        }
        _ => Err(bad()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::to_text;

    #[test]
    fn round_trips_a_small_game() {
        let text = "\
% trick-notation v1
[Game \"spades\"]
[Deck \"french52\"]
[Seats \"N E S W\"]
[Dealer \"N\"]
[Players \"Ann Bo Cy Di\"]
[MaxPoints \"250\"]

D N:AK.T.-.-
C E: 3 4 nil 4
P E KC 5C
";
        let model = from_text(text).expect("parse");
        assert_eq!(model.meta.game_hint.as_deref(), Some("spades"));
        assert_eq!(model.meta.seats, vec!["N", "E", "S", "W"]);
        assert_eq!(model.meta.extra, vec![("MaxPoints".to_string(), "250".to_string())]);
        assert_eq!(model.events.len(), 3);
        // Re-encoding the parsed model reproduces the input exactly.
        assert_eq!(to_text(&model), text);
    }

    #[test]
    fn rejects_missing_version() {
        assert_eq!(from_text("[Game \"x\"]\n"), Err(ParseError::MissingVersion));
    }
}
