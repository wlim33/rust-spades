use std::fmt::Write as _;

use crate::{Game, State};

use super::format::{card_to_str, escape_tag_value};

pub fn encode(game: &Game) -> String {
    let mut out = String::with_capacity(1024);
    encode_headers(&mut out, game);
    out.push('\n');
    encode_rounds(&mut out, game);
    out
}

fn encode_headers(out: &mut String, g: &Game) {
    writeln!(out, "[GameId \"{}\"]", g.get_id()).unwrap();
    writeln!(out, "[MaxPoints \"{}\"]", g.get_max_points()).unwrap();

    let names = g.get_player_names();
    for i in 0..4 {
        writeln!(out, "[Player{} \"{}\"]", i, names[i].0).unwrap();
    }
    for i in 0..4 {
        if let Some(n) = names[i].1 {
            writeln!(out, "[Name{} \"{}\"]", i, escape_tag_value(n)).unwrap();
        }
    }

    if let Some(t) = g.get_timer_config() {
        writeln!(out, "[TimerInitial \"{}\"]", t.initial_time_secs).unwrap();
        writeln!(out, "[TimerIncrement \"{}\"]", t.increment_secs).unwrap();
    }

    let termination = match g.get_state() {
        State::Completed => "Completed",
        State::Aborted => "Aborted",
        _ => "InProgress",
    };
    writeln!(out, "[Termination \"{}\"]", termination).unwrap();

    let result = match g.get_state() {
        State::Completed | State::Aborted => {
            let a = g.get_team_a_score().copied().unwrap_or(0);
            let b = g.get_team_b_score().copied().unwrap_or(0);
            format!("{}-{}", a, b)
        }
        _ => "*".to_string(),
    };
    writeln!(out, "[Result \"{}\"]", result).unwrap();
}

fn encode_rounds(_out: &mut String, _g: &Game) {
    // Implemented in task 6.
}

// `card_to_str` is imported here so task 6 can use it without re-adding
// the import. Suppress the unused-import warning until then.
#[allow(dead_code)]
fn _ensure_card_to_str_in_scope(_c: crate::cards::Card) {
    let _ = card_to_str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Game, TimerConfig};
    use uuid::Uuid;

    fn fixed_uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    #[test]
    fn header_not_started_no_names_no_timer() {
        let g = Game::new(
            fixed_uuid(1),
            [fixed_uuid(10), fixed_uuid(11), fixed_uuid(12), fixed_uuid(13)],
            500,
            None,
        );
        let s = encode(&g);
        let expected = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]
\n";
        assert_eq!(s, expected);
    }

    #[test]
    fn header_with_names_and_timer() {
        let mut g = Game::new(
            fixed_uuid(1),
            [fixed_uuid(10), fixed_uuid(11), fixed_uuid(12), fixed_uuid(13)],
            300,
            Some(TimerConfig { initial_time_secs: 300, increment_secs: 5 }),
        );
        g.set_player_name(fixed_uuid(10), Some("Alice".into())).unwrap();
        g.set_player_name(fixed_uuid(12), Some("Carol \"Q\"".into())).unwrap();
        let s = encode(&g);
        assert!(s.contains("[Name0 \"Alice\"]\n"));
        assert!(s.contains("[Name2 \"Carol \\\"Q\\\"\"]\n"));
        assert!(!s.contains("[Name1 "));
        assert!(!s.contains("[Name3 "));
        assert!(s.contains("[TimerInitial \"300\"]\n"));
        assert!(s.contains("[TimerIncrement \"5\"]\n"));
    }
}
