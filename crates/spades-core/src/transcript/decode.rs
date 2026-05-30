use uuid::Uuid;

use crate::TimerConfig;

use super::format::{parse_card, unescape_tag_value};
use super::{DecodeError, Headers, Round, Termination, Transcript};

/// Parse a transcript text into a structured `Transcript`.
///
/// This function performs *syntactic* validation only: it confirms the format
/// is well-formed (tag pairs, escape sequences, card notation, monotonic round
/// numbers, count bounds). It does NOT verify that the encoded moves form a
/// legal game — that semantic check happens in [`replay`].
///
/// Returns specific `DecodeError` variants pointing to the offending line for
/// every failure mode.
pub fn decode(text: &str) -> Result<Transcript, DecodeError> {
    let mut parser = Parser::new(text);
    let headers = parser.parse_headers()?;
    let (termination, result) = parser.consume_termination_and_result()?;
    let rounds = parser.parse_rounds()?;
    parser.expect_eof()?;
    Ok(Transcript {
        headers,
        rounds,
        termination,
        result,
    })
}

struct Parser<'a> {
    lines: Vec<(usize, &'a str)>, // (1-based line number, content)
    cursor: usize,
    termination: Option<Termination>,
    result: Option<(i32, i32)>,
    result_was_star: bool,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        let lines = text
            .split('\n')
            .enumerate()
            .map(|(i, l)| (i + 1, l))
            .collect();
        Parser {
            lines,
            cursor: 0,
            termination: None,
            result: None,
            result_was_star: false,
        }
    }

    fn peek(&self) -> Option<&(usize, &'a str)> {
        self.lines.get(self.cursor)
    }

    fn advance(&mut self) -> Option<&(usize, &'a str)> {
        let v = self.lines.get(self.cursor);
        self.cursor += 1;
        v
    }

    fn parse_headers(&mut self) -> Result<Headers, DecodeError> {
        let mut game_id: Option<Uuid> = None;
        let mut max_points: Option<i32> = None;
        let mut player_ids: [Option<Uuid>; 4] = [None; 4];
        let mut names: [Option<String>; 4] = Default::default();
        let mut timer_initial: Option<u64> = None;
        let mut timer_increment: Option<u64> = None;

        loop {
            let Some((ln, line)) = self.peek().copied() else {
                return Err(DecodeError::UnexpectedEof);
            };
            if line.is_empty() {
                self.advance();
                break;
            }
            if !line.starts_with('[') {
                return Err(DecodeError::BadTag {
                    line: ln,
                    found: line.to_string(),
                });
            }
            let (key, value) = parse_tag_line(ln, line)?;
            self.advance();
            match key.as_str() {
                "GameId" => set_once(&mut game_id, parse_uuid(ln, &value)?, ln, "GameId")?,
                "MaxPoints" => set_once(&mut max_points, parse_int(ln, &value)?, ln, "MaxPoints")?,
                "Player0" => set_once(&mut player_ids[0], parse_uuid(ln, &value)?, ln, "Player0")?,
                "Player1" => set_once(&mut player_ids[1], parse_uuid(ln, &value)?, ln, "Player1")?,
                "Player2" => set_once(&mut player_ids[2], parse_uuid(ln, &value)?, ln, "Player2")?,
                "Player3" => set_once(&mut player_ids[3], parse_uuid(ln, &value)?, ln, "Player3")?,
                "Name0" => set_once(&mut names[0], value, ln, "Name0")?,
                "Name1" => set_once(&mut names[1], value, ln, "Name1")?,
                "Name2" => set_once(&mut names[2], value, ln, "Name2")?,
                "Name3" => set_once(&mut names[3], value, ln, "Name3")?,
                "TimerInitial" => set_once(
                    &mut timer_initial,
                    parse_u64(ln, &value)?,
                    ln,
                    "TimerInitial",
                )?,
                "TimerIncrement" => set_once(
                    &mut timer_increment,
                    parse_u64(ln, &value)?,
                    ln,
                    "TimerIncrement",
                )?,
                "Termination" => {
                    let t = match value.as_str() {
                        "Completed" => Termination::Completed,
                        "Aborted" => Termination::Aborted,
                        "InProgress" => Termination::InProgress,
                        _ => return Err(DecodeError::BadTermination { line: ln, value }),
                    };
                    if self.termination.is_some() {
                        return Err(DecodeError::DuplicateTag {
                            line: ln,
                            key: "Termination".into(),
                        });
                    }
                    self.termination = Some(t);
                }
                "Result" => {
                    if self.result.is_some() || self.result_was_star {
                        return Err(DecodeError::DuplicateTag {
                            line: ln,
                            key: "Result".into(),
                        });
                    }
                    if value == "*" {
                        self.result_was_star = true;
                    } else {
                        let parts: Vec<&str> = value.split_whitespace().collect();
                        if parts.len() != 2 {
                            return Err(DecodeError::BadResult {
                                line: ln,
                                value: value.clone(),
                            });
                        }
                        let a = parts[0]
                            .parse::<i32>()
                            .map_err(|_| DecodeError::BadResult {
                                line: ln,
                                value: value.clone(),
                            })?;
                        let b = parts[1]
                            .parse::<i32>()
                            .map_err(|_| DecodeError::BadResult {
                                line: ln,
                                value: value.clone(),
                            })?;
                        self.result = Some((a, b));
                    }
                }
                _ => {
                    return Err(DecodeError::BadTag {
                        line: ln,
                        found: line.to_string(),
                    });
                }
            }
        }

        let game_id = game_id.ok_or(DecodeError::MissingRequiredTag { key: "GameId" })?;
        let max_points = max_points.ok_or(DecodeError::MissingRequiredTag { key: "MaxPoints" })?;
        let player_ids = [
            player_ids[0].ok_or(DecodeError::MissingRequiredTag { key: "Player0" })?,
            player_ids[1].ok_or(DecodeError::MissingRequiredTag { key: "Player1" })?,
            player_ids[2].ok_or(DecodeError::MissingRequiredTag { key: "Player2" })?,
            player_ids[3].ok_or(DecodeError::MissingRequiredTag { key: "Player3" })?,
        ];
        let timer = match (timer_initial, timer_increment) {
            (Some(a), Some(b)) => Some(TimerConfig {
                initial_time_secs: a,
                increment_secs: b,
            }),
            (None, None) => None,
            _ => {
                return Err(DecodeError::MissingRequiredTag {
                    key: "TimerInitial/Increment pair",
                });
            }
        };
        Ok(Headers {
            game_id,
            max_points,
            player_ids,
            names,
            timer,
        })
    }

    fn consume_termination_and_result(
        &mut self,
    ) -> Result<(Termination, Option<(i32, i32)>), DecodeError> {
        let term = self
            .termination
            .ok_or(DecodeError::MissingRequiredTag { key: "Termination" })?;
        if !self.result_was_star && self.result.is_none() {
            return Err(DecodeError::MissingRequiredTag { key: "Result" });
        }
        Ok((term, self.result))
    }

    fn parse_rounds(&mut self) -> Result<Vec<Round>, DecodeError> {
        let mut rounds: Vec<Round> = Vec::new();
        while let Some((ln, line)) = self.peek().copied() {
            if line.is_empty() {
                self.advance();
                continue;
            }
            if !line.starts_with("[Round ") {
                break;
            }

            let (_key, value) = parse_tag_line(ln, line)?;
            self.advance();
            let round_num = parse_int(ln, &value)?;
            if round_num < 1 {
                return Err(DecodeError::BadInteger { line: ln, value });
            }
            let expected = rounds.len() + 1;
            if (round_num as usize) != expected {
                return Err(DecodeError::NonMonotonicRound {
                    expected,
                    found: round_num as usize,
                });
            }

            // 4 [HandN] lines
            let mut hands: [Vec<crate::cards::Card>; 4] = Default::default();
            for (seat, hand) in hands.iter_mut().enumerate() {
                let (ln2, line2) = self.advance().copied().ok_or(DecodeError::UnexpectedEof)?;
                let (k, v) = parse_tag_line(ln2, line2)?;
                if k != format!("Hand{}", seat) {
                    return Err(DecodeError::BadTag {
                        line: ln2,
                        found: line2.to_string(),
                    });
                }
                for tok in v.split_whitespace() {
                    let c = parse_card(tok).ok_or(DecodeError::BadCard {
                        line: ln2,
                        token: tok.to_string(),
                    })?;
                    hand.push(c);
                }
            }

            // [Bets "..."]
            let (ln3, line3) = self.advance().copied().ok_or(DecodeError::UnexpectedEof)?;
            let (k3, v3) = parse_tag_line(ln3, line3)?;
            if k3 != "Bets" {
                return Err(DecodeError::BadTag {
                    line: ln3,
                    found: line3.to_string(),
                });
            }
            let bets: Vec<i32> = if v3.is_empty() {
                Vec::new()
            } else {
                let mut out = Vec::new();
                for tok in v3.split_whitespace() {
                    out.push(parse_int(ln3, tok)?);
                }
                out
            };
            if bets.len() > 4 {
                return Err(DecodeError::TooManyBets {
                    round: round_num as usize,
                });
            }

            // Trick lines until blank/next [Round]/EOF
            let mut tricks: Vec<Vec<crate::cards::Card>> = Vec::new();
            while let Some((ln4, line4)) = self.peek().copied() {
                if line4.is_empty() || line4.starts_with('[') {
                    break;
                }
                let (num_str, rest) = line4.split_once('.').ok_or_else(|| DecodeError::BadTag {
                    line: ln4,
                    found: line4.to_string(),
                })?;
                let trick_num = parse_int(ln4, num_str)?;
                let expected_t = tricks.len() + 1;
                if (trick_num as usize) != expected_t {
                    return Err(DecodeError::BadTag {
                        line: ln4,
                        found: line4.to_string(),
                    });
                }
                let mut cards: Vec<crate::cards::Card> = Vec::new();
                for tok in rest.split_whitespace() {
                    let c = parse_card(tok).ok_or(DecodeError::BadCard {
                        line: ln4,
                        token: tok.to_string(),
                    })?;
                    cards.push(c);
                }
                if cards.is_empty() {
                    return Err(DecodeError::BadTag {
                        line: ln4,
                        found: line4.to_string(),
                    });
                }
                if cards.len() > 4 {
                    return Err(DecodeError::TooManyCardsInTrick {
                        round: round_num as usize,
                        trick: trick_num as usize,
                    });
                }
                self.advance();
                tricks.push(cards);
                if tricks.len() > 13 {
                    return Err(DecodeError::TooManyTricks {
                        round: round_num as usize,
                    });
                }
            }

            rounds.push(Round {
                hands,
                bets,
                tricks,
            });
        }
        Ok(rounds)
    }

    fn expect_eof(&mut self) -> Result<(), DecodeError> {
        // `parse_rounds` advances past any empty lines, so the cursor is
        // either at EOF or at a non-empty, non-`[Round` line.
        match self.peek().copied() {
            None => Ok(()),
            Some((ln, _)) => Err(DecodeError::TrailingContent { line: ln }),
        }
    }
}

fn parse_tag_line(line_no: usize, line: &str) -> Result<(String, String), DecodeError> {
    // Expect: [<Key> "<Value>"]
    let inside = line
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| DecodeError::BadTag {
            line: line_no,
            found: line.to_string(),
        })?;
    let (key, rest) = inside.split_once(' ').ok_or_else(|| DecodeError::BadTag {
        line: line_no,
        found: line.to_string(),
    })?;
    let value = rest
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| DecodeError::BadTag {
            line: line_no,
            found: line.to_string(),
        })?;
    let unescaped = unescape_tag_value(value).ok_or_else(|| DecodeError::BadEscape {
        line: line_no,
        value: value.to_string(),
    })?;
    Ok((key.to_string(), unescaped))
}

fn set_once<T>(slot: &mut Option<T>, value: T, ln: usize, key: &str) -> Result<(), DecodeError> {
    if slot.is_some() {
        return Err(DecodeError::DuplicateTag {
            line: ln,
            key: key.to_string(),
        });
    }
    *slot = Some(value);
    Ok(())
}

fn parse_uuid(ln: usize, v: &str) -> Result<Uuid, DecodeError> {
    Uuid::parse_str(v).map_err(|_| DecodeError::BadUuid {
        line: ln,
        value: v.to_string(),
    })
}

fn parse_int(ln: usize, v: &str) -> Result<i32, DecodeError> {
    v.parse::<i32>().map_err(|_| DecodeError::BadInteger {
        line: ln,
        value: v.to_string(),
    })
}

fn parse_u64(ln: usize, v: &str) -> Result<u64, DecodeError> {
    v.parse::<u64>().map_err(|_| DecodeError::BadInteger {
        line: ln,
        value: v.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn u(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    #[test]
    fn decode_minimal_header() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        let t = decode(s).unwrap();
        assert_eq!(t.headers.game_id, u(1));
        assert_eq!(t.headers.max_points, 500);
        assert_eq!(t.headers.player_ids, [u(0x0a), u(0x0b), u(0x0c), u(0x0d)]);
        assert_eq!(t.headers.names, [None, None, None, None]);
        assert!(t.headers.timer.is_none());
        assert_eq!(t.termination, Termination::InProgress);
        assert!(t.result.is_none());
    }

    #[test]
    fn decode_header_with_names_and_timer() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"300\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Name0 \"Alice\"]
[Name2 \"Carol \\\"Q\\\"\"]
[TimerInitial \"300\"]
[TimerIncrement \"5\"]
[Termination \"Completed\"]
[Result \"520 430\"]
";
        let t = decode(s).unwrap();
        assert_eq!(t.headers.names[0].as_deref(), Some("Alice"));
        assert_eq!(t.headers.names[1], None);
        assert_eq!(t.headers.names[2].as_deref(), Some("Carol \"Q\""));
        assert_eq!(t.headers.names[3], None);
        assert_eq!(
            t.headers
                .timer
                .map(|tc| (tc.initial_time_secs, tc.increment_secs)),
            Some((300, 5))
        );
        assert_eq!(t.termination, Termination::Completed);
        assert_eq!(t.result, Some((520, 430)));
    }

    #[test]
    fn decode_rejects_missing_required_tag() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::MissingRequiredTag { key: "Player2" })
        ));
    }

    #[test]
    fn decode_rejects_duplicate_tag() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[MaxPoints \"600\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::DuplicateTag { key, .. }) if key == "MaxPoints"
        ));
    }

    #[test]
    fn decode_rejects_bad_uuid() {
        let s = "\
[GameId \"not-a-uuid\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadUuid { .. })));
    }

    use crate::cards::{Card, Rank, Suit};

    fn c(r: Rank, su: Suit) -> Card {
        Card { rank: r, suit: su }
    }

    #[test]
    fn decode_one_round_block() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"3 4 2 4\"]
1. 2C 5C 7C TC
";
        let t = decode(s).unwrap();
        assert_eq!(t.rounds.len(), 1);
        let r = &t.rounds[0];
        assert_eq!(r.hands[0].len(), 13);
        assert_eq!(r.hands[0][0], c(Rank::Two, Suit::Club));
        assert_eq!(r.bets, vec![3, 4, 2, 4]);
        assert_eq!(r.tricks.len(), 1);
        assert_eq!(
            r.tricks[0],
            vec![
                c(Rank::Two, Suit::Club),
                c(Rank::Five, Suit::Club),
                c(Rank::Seven, Suit::Club),
                c(Rank::Ten, Suit::Club),
            ]
        );
    }

    #[test]
    fn decode_partial_bets() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"3 4\"]
";
        let t = decode(s).unwrap();
        assert_eq!(t.rounds[0].bets, vec![3, 4]);
        assert!(t.rounds[0].tricks.is_empty());
    }

    #[test]
    fn decode_rejects_non_monotonic_round() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"3\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"\"]
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::NonMonotonicRound {
                expected: 1,
                found: 3
            })
        ));
    }

    #[test]
    fn decode_rejects_too_many_bets() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"3 4 2 4 1\"]
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::TooManyBets { round: 1 })
        ));
    }

    #[test]
    fn decode_rejects_too_many_cards_in_trick() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"3 4 2 4\"]
1. 2C 5C 7C TC 8C
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::TooManyCardsInTrick { round: 1, trick: 1 })
        ));
    }

    #[test]
    fn decode_rejects_bad_card() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S 1X\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadCard { token, .. }) if token == "1X"));
    }

    #[test]
    fn decode_rejects_bad_termination() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"Forfeit\"]
[Result \"*\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadTermination { .. })));
    }

    #[test]
    fn decode_rejects_bad_result() {
        // After the Task-9 format change, Result is space-separated. A single-token
        // value with no space is invalid.
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"Completed\"]
[Result \"100\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadResult { .. })));
    }

    #[test]
    fn decode_rejects_unknown_tag() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[Mystery \"x\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadTag { .. })));
    }

    #[test]
    fn decode_rejects_trailing_content() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

garbage
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::TrailingContent { .. })
        ));
    }

    #[test]
    fn decode_rejects_bad_escape() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Name0 \"A\\nB\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadEscape { .. })));
    }

    #[test]
    fn decode_rejects_timer_half_specified() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[TimerInitial \"300\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::MissingRequiredTag { key }) if key == "TimerInitial/Increment pair"
        ));
    }

    #[test]
    fn decode_rejects_unexpected_eof_in_header() {
        // No trailing newline, no blank-line terminator: header parser runs
        // out of lines before encountering the header/round separator.
        let s = "[GameId \"01010101-0101-0101-0101-010101010101\"]\n[MaxPoints \"500\"]";
        assert!(matches!(decode(s), Err(DecodeError::UnexpectedEof)));
    }

    #[test]
    fn decode_rejects_non_bracket_line_in_header() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
not a tag at all
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        assert!(
            matches!(decode(s), Err(DecodeError::BadTag { found, .. }) if found == "not a tag at all")
        );
    }

    #[test]
    fn decode_rejects_duplicate_termination() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Termination \"Completed\"]
[Result \"*\"]
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::DuplicateTag { key, .. }) if key == "Termination"
        ));
    }

    #[test]
    fn decode_rejects_duplicate_result() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]
[Result \"*\"]
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::DuplicateTag { key, .. }) if key == "Result"
        ));
    }

    #[test]
    fn decode_rejects_missing_result() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::MissingRequiredTag { key: "Result" })
        ));
    }

    #[test]
    fn decode_rejects_round_zero() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"0\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadInteger { value, .. }) if value == "0"));
    }

    #[test]
    fn decode_rejects_wrong_seat_hand() {
        // [Hand1] appears in the Hand0 slot.
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand1 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadTag { .. })));
    }

    #[test]
    fn decode_rejects_wrong_bets_key() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Beets \"3 4 2 4\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadTag { .. })));
    }

    #[test]
    fn decode_rejects_out_of_order_trick() {
        // First trick line is numbered 2, not 1.
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"3 4 2 4\"]
2. 2C 5C 7C TC
";
        assert!(matches!(decode(s), Err(DecodeError::BadTag { .. })));
    }

    #[test]
    fn decode_rejects_empty_trick_line() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"3 4 2 4\"]
1.
";
        assert!(matches!(decode(s), Err(DecodeError::BadTag { .. })));
    }

    #[test]
    fn decode_rejects_too_many_tricks() {
        // 14 trick lines in one round (max is 13).
        let mut s = String::from(
            "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"3 4 2 4\"]
",
        );
        for i in 1..=14 {
            s.push_str(&format!("{}. 2C 5C 7C TC\n", i));
        }
        assert!(matches!(
            decode(&s),
            Err(DecodeError::TooManyTricks { round: 1 })
        ));
    }
}
