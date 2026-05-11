use uuid::Uuid;

use crate::TimerConfig;

use super::format::{parse_card, unescape_tag_value};
use super::{DecodeError, Headers, Round, Termination, Transcript};

pub fn decode(text: &str) -> Result<Transcript, DecodeError> {
    let mut parser = Parser::new(text);
    let headers = parser.parse_headers()?;
    let (termination, result) = parser.consume_termination_and_result()?;
    let rounds = parser.parse_rounds()?;
    parser.expect_eof()?;
    Ok(Transcript { headers, rounds, termination, result })
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
        Parser { lines, cursor: 0, termination: None, result: None, result_was_star: false }
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
                return Err(DecodeError::BadTag { line: ln, found: line.to_string() });
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
                "TimerInitial" => set_once(&mut timer_initial, parse_u64(ln, &value)?, ln, "TimerInitial")?,
                "TimerIncrement" => set_once(&mut timer_increment, parse_u64(ln, &value)?, ln, "TimerIncrement")?,
                "Termination" => {
                    let t = match value.as_str() {
                        "Completed" => Termination::Completed,
                        "Aborted" => Termination::Aborted,
                        "InProgress" => Termination::InProgress,
                        _ => return Err(DecodeError::BadTermination { line: ln, value }),
                    };
                    if self.termination.is_some() {
                        return Err(DecodeError::DuplicateTag { line: ln, key: "Termination".into() });
                    }
                    self.termination = Some(t);
                }
                "Result" => {
                    if self.result.is_some() || self.result_was_star {
                        return Err(DecodeError::DuplicateTag { line: ln, key: "Result".into() });
                    }
                    if value == "*" {
                        self.result_was_star = true;
                    } else {
                        let (a_str, b_str) = value
                            .split_once('-')
                            .ok_or_else(|| DecodeError::BadResult { line: ln, value: value.clone() })?;
                        let a = a_str.parse::<i32>().map_err(|_| DecodeError::BadResult { line: ln, value: value.clone() })?;
                        let b = b_str.parse::<i32>().map_err(|_| DecodeError::BadResult { line: ln, value: value.clone() })?;
                        self.result = Some((a, b));
                    }
                }
                _ => return Err(DecodeError::BadTag { line: ln, found: line.to_string() }),
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
            (Some(a), Some(b)) => Some(TimerConfig { initial_time_secs: a, increment_secs: b }),
            (None, None) => None,
            _ => return Err(DecodeError::MissingRequiredTag { key: "TimerInitial/Increment pair" }),
        };
        Ok(Headers { game_id, max_points, player_ids, names, timer })
    }

    fn consume_termination_and_result(&mut self) -> Result<(Termination, Option<(i32, i32)>), DecodeError> {
        let term = self.termination.ok_or(DecodeError::MissingRequiredTag { key: "Termination" })?;
        if !self.result_was_star && self.result.is_none() {
            return Err(DecodeError::MissingRequiredTag { key: "Result" });
        }
        Ok((term, self.result))
    }

    fn parse_rounds(&mut self) -> Result<Vec<Round>, DecodeError> {
        // Implemented in task 8.
        Ok(Vec::new())
    }

    fn expect_eof(&mut self) -> Result<(), DecodeError> {
        while let Some((_, l)) = self.peek() {
            if l.is_empty() {
                self.advance();
            } else {
                let (ln, _) = self.peek().copied().unwrap();
                return Err(DecodeError::TrailingContent { line: ln });
            }
        }
        Ok(())
    }
}

fn parse_tag_line(line_no: usize, line: &str) -> Result<(String, String), DecodeError> {
    // Expect: [<Key> "<Value>"]
    let inside = line
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| DecodeError::BadTag { line: line_no, found: line.to_string() })?;
    let (key, rest) = inside
        .split_once(' ')
        .ok_or_else(|| DecodeError::BadTag { line: line_no, found: line.to_string() })?;
    let value = rest
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| DecodeError::BadTag { line: line_no, found: line.to_string() })?;
    let unescaped = unescape_tag_value(value)
        .ok_or_else(|| DecodeError::BadEscape { line: line_no, value: value.to_string() })?;
    Ok((key.to_string(), unescaped))
}

fn set_once<T>(slot: &mut Option<T>, value: T, ln: usize, key: &str) -> Result<(), DecodeError> {
    if slot.is_some() {
        return Err(DecodeError::DuplicateTag { line: ln, key: key.to_string() });
    }
    *slot = Some(value);
    Ok(())
}

fn parse_uuid(ln: usize, v: &str) -> Result<Uuid, DecodeError> {
    Uuid::parse_str(v).map_err(|_| DecodeError::BadUuid { line: ln, value: v.to_string() })
}

fn parse_int(ln: usize, v: &str) -> Result<i32, DecodeError> {
    v.parse::<i32>().map_err(|_| DecodeError::BadInteger { line: ln, value: v.to_string() })
}

fn parse_u64(ln: usize, v: &str) -> Result<u64, DecodeError> {
    v.parse::<u64>().map_err(|_| DecodeError::BadInteger { line: ln, value: v.to_string() })
}

// Keep `parse_card` in scope; task 8 will use it for trick lines.
#[allow(dead_code)]
fn _silence_parse_card_warning(_: fn(&str) -> Option<crate::cards::Card>) {
    let _ = parse_card;
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
[Result \"520-430\"]
";
        let t = decode(s).unwrap();
        assert_eq!(t.headers.names[0].as_deref(), Some("Alice"));
        assert_eq!(t.headers.names[1], None);
        assert_eq!(t.headers.names[2].as_deref(), Some("Carol \"Q\""));
        assert_eq!(t.headers.names[3], None);
        assert_eq!(t.headers.timer.map(|tc| (tc.initial_time_secs, tc.increment_secs)), Some((300, 5)));
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
}
