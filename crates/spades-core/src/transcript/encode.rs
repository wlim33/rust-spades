use std::fmt::Write as _;

use crate::cards::{Card, get_trick_winner};
use crate::{Game, State};

use super::format::{card_to_str, escape_tag_value};

/// Serialize a `Game` to its Spades Transcript Format (STF) representation.
///
/// This function is total — every valid `Game` produces a valid transcript,
/// including mid-game states (NotStarted, Betting, Trick) and terminal states
/// (Completed, Aborted). The output is deterministic for a given `Game`: the
/// same state always produces byte-equal output.
///
/// For round-trip use:
/// ```text
/// encode(replay(decode(s))?) == s
/// ```
/// holds for any well-formed transcript `s`.
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
    for (i, name) in names.iter().enumerate() {
        writeln!(out, "[Player{} \"{}\"]", i, name.0).unwrap();
    }
    for (i, name) in names.iter().enumerate() {
        if let Some(n) = name.1 {
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
            format!("{} {}", a, b)
        }
        _ => "*".to_string(),
    };
    writeln!(out, "[Result \"{}\"]", result).unwrap();
}

fn encode_rounds(out: &mut String, g: &Game) {
    let num_rounds = num_rounds_to_emit(g);
    if num_rounds == 0 {
        return;
    }
    for r in 0..num_rounds {
        if r > 0 {
            out.push('\n');
        }
        encode_round(out, g, r);
    }
}

/// Number of round blocks to emit.
fn num_rounds_to_emit(g: &Game) -> usize {
    match g.get_state() {
        State::NotStarted => 0,
        State::Completed => g.get_round_index(),
        State::Aborted => {
            // Aborted from NotStarted: no rounds were started.
            let history = g.get_history();
            let no_play =
                history.len() <= 1 && history.iter().all(|t| t.iter().all(|c| c.is_none()));
            let no_bets = g.get_all_bets().first().copied().unwrap_or([0; 4]) == [0; 4]
                && g.get_round_index() == 0
                && g.is_in_betting_stage();
            if no_play && no_bets {
                0
            } else {
                g.get_round_index() + 1
            }
        }
        State::Betting(_) | State::Trick(_) => g.get_round_index() + 1,
    }
}

fn encode_round(out: &mut String, g: &Game, round_idx: usize) {
    writeln!(out, "[Round \"{}\"]", round_idx + 1).unwrap();

    let hands = dealt_hands_for_round(g, round_idx);
    for (seat, hand) in hands.iter().enumerate() {
        write!(out, "[Hand{} \"", seat).unwrap();
        let mut first = true;
        for c in hand {
            if !first {
                out.push(' ');
            }
            first = false;
            let b = card_to_str(*c);
            out.push(b[0] as char);
            out.push(b[1] as char);
        }
        out.push_str("\"]\n");
    }

    let bets = bets_for_round(g, round_idx);
    write!(out, "[Bets \"").unwrap();
    let mut first = true;
    for b in &bets {
        if !first {
            out.push(' ');
        }
        first = false;
        write!(out, "{}", b).unwrap();
    }
    out.push_str("\"]\n");

    let tricks = tricks_for_round(g, round_idx);
    for (t, trick_cards) in tricks.iter().enumerate() {
        write!(out, "{}.", t + 1).unwrap();
        for c in trick_cards {
            out.push(' ');
            let b = card_to_str(*c);
            out.push(b[0] as char);
            out.push(b[1] as char);
        }
        out.push('\n');
    }
}

/// Reconstruct the dealt hand per seat at the start of round `round_idx`.
fn dealt_hands_for_round(g: &Game, round_idx: usize) -> [Vec<Card>; 4] {
    let history = g.get_history();
    let start = 13 * round_idx;
    let end = (start + 13).min(history.len());
    let trick_slots = &history[start..end];

    let mut hands: [Vec<Card>; 4] = Default::default();
    for trick in trick_slots {
        for (seat, slot) in trick.iter().enumerate() {
            if let Some(c) = slot {
                hands[seat].push(*c);
            }
        }
    }

    // For the current round (mid-game), include cards still in each player's
    // hand. For past completed rounds, the engine has already dealt the next
    // round's cards into players' hands, so we must NOT pull from current hand.
    let is_current_round = match g.get_state() {
        State::Betting(_) | State::Trick(_) => g.get_round_index() == round_idx,
        State::Aborted => g.get_round_index() == round_idx,
        _ => false,
    };
    if is_current_round {
        let names = g.get_player_names();
        for seat in 0..4 {
            let pid = names[seat].0;
            if let Ok(hand) = g.get_hand_by_player_id(pid) {
                for c in hand {
                    hands[seat].push(*c);
                }
            }
        }
    }

    for h in &mut hands {
        h.sort();
    }
    hands
}

/// Bets to emit for round `round_idx`. May be 0..=4 entries.
fn bets_for_round(g: &Game, round_idx: usize) -> Vec<i32> {
    let all = g.get_all_bets();
    let row = all.get(round_idx).copied().unwrap_or([0; 4]);
    let count = match g.get_state() {
        State::Betting(k) if g.get_round_index() == round_idx => *k,
        State::Aborted if g.get_round_index() == round_idx && g.is_in_betting_stage() => {
            // Cannot recover k from an Aborted-betting state precisely.
            // Emit all 4: under-reporting silently drops info; over-reporting
            // surfaces as a replay error later if the trailing entries weren't
            // actually placed by any user.
            4
        }
        _ => 4,
    };
    row[..count].to_vec()
}

/// Tricks for round `round_idx` in play order. Each inner Vec has 1..=4 cards;
/// the last may be partial. Empty slots (trailing placeholder from history
/// retention fix during betting of a later round) are dropped — but the caller
/// `num_rounds_to_emit` already gates emission so empty slots only appear in
/// rounds we'd otherwise skip.
fn tricks_for_round(g: &Game, round_idx: usize) -> Vec<Vec<Card>> {
    let history = g.get_history();
    let start = 13 * round_idx;
    let end = (start + 13).min(history.len());
    let mut out = Vec::new();
    let mut lead = 0usize;
    for trick in &history[start..end] {
        let count = trick.iter().filter(|c| c.is_some()).count();
        if count == 0 {
            continue;
        }
        let mut play_order = Vec::with_capacity(count);
        for i in 0..4 {
            let seat = (lead + i) % 4;
            if let Some(c) = trick[seat] {
                play_order.push(c);
            } else {
                break;
            }
        }
        if count == 4 {
            let by_seat: [Card; 4] = [
                trick[0].unwrap(),
                trick[1].unwrap(),
                trick[2].unwrap(),
                trick[3].unwrap(),
            ];
            lead = get_trick_winner(lead, &by_seat);
        }
        out.push(play_order);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Game, GameTransition, TimerConfig};
    use uuid::Uuid;

    fn fixed_uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    #[test]
    fn header_not_started_no_names_no_timer() {
        let g = Game::new(
            fixed_uuid(1),
            [
                fixed_uuid(10),
                fixed_uuid(11),
                fixed_uuid(12),
                fixed_uuid(13),
            ],
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
            [
                fixed_uuid(10),
                fixed_uuid(11),
                fixed_uuid(12),
                fixed_uuid(13),
            ],
            300,
            Some(TimerConfig {
                initial_time_secs: 300,
                increment_secs: 5,
            }),
        );
        g.set_player_name(fixed_uuid(10), Some("Alice".into()))
            .unwrap();
        g.set_player_name(fixed_uuid(12), Some("Carol \"Q\"".into()))
            .unwrap();
        let s = encode(&g);
        assert!(s.contains("[Name0 \"Alice\"]\n"));
        assert!(s.contains("[Name2 \"Carol \\\"Q\\\"\"]\n"));
        assert!(!s.contains("[Name1 "));
        assert!(!s.contains("[Name3 "));
        assert!(s.contains("[TimerInitial \"300\"]\n"));
        assert!(s.contains("[TimerIncrement \"5\"]\n"));
    }

    /// Drive a Game forward by always choosing the first legal card / Bet(3) /
    /// Start. Used to construct deterministic-ish game states for tests.
    fn play_first_legal(g: &mut Game, transitions: usize) {
        for _ in 0..transitions {
            match g.get_state() {
                State::NotStarted => {
                    g.play(GameTransition::Start).unwrap();
                }
                State::Betting(_) => {
                    g.play(GameTransition::Bet(3)).unwrap();
                }
                State::Trick(_) => {
                    let legal = g.get_legal_cards().unwrap();
                    g.play(GameTransition::Card(legal[0])).unwrap();
                }
                State::Completed | State::Aborted => return,
            }
        }
    }

    #[test]
    fn encode_mid_first_bet() {
        let mut g = Game::new(
            fixed_uuid(1),
            [
                fixed_uuid(10),
                fixed_uuid(11),
                fixed_uuid(12),
                fixed_uuid(13),
            ],
            500,
            None,
        );
        play_first_legal(&mut g, 1); // Start
        play_first_legal(&mut g, 2); // 2 bets

        let s = encode(&g);
        assert!(s.contains("[Round \"1\"]\n"), "should have Round 1 block");
        assert!(
            s.contains("[Bets \"3 3\"]\n"),
            "should have 2 bets, got:\n{}",
            s
        );
        for line in s.lines() {
            assert!(
                !line.starts_with("1. ") && !line.starts_with("2. "),
                "unexpected trick line: {}",
                line
            );
        }
    }

    #[test]
    fn encode_completed_short_game() {
        // Low max_points so the game finishes in 1-2 rounds.
        let mut g = Game::new(
            fixed_uuid(1),
            [
                fixed_uuid(10),
                fixed_uuid(11),
                fixed_uuid(12),
                fixed_uuid(13),
            ],
            50,
            None,
        );
        play_first_legal(&mut g, 10_000);
        assert_eq!(*g.get_state(), State::Completed);

        let s = encode(&g);
        assert!(s.contains("[Termination \"Completed\"]\n"));
        assert!(s.contains("[Round \"1\"]\n"));

        let result_line = s.lines().find(|l| l.starts_with("[Result \"")).unwrap();
        assert!(result_line.contains(" "));
        assert_ne!(result_line, "[Result \"*\"]");

        for seat in 0..4 {
            let tag = format!("[Hand{} \"", seat);
            assert!(s.contains(&tag), "Hand{} not present", seat);
        }
    }

    #[test]
    fn hands_are_sorted() {
        let mut g = Game::new(
            fixed_uuid(1),
            [
                fixed_uuid(10),
                fixed_uuid(11),
                fixed_uuid(12),
                fixed_uuid(13),
            ],
            500,
            None,
        );
        play_first_legal(&mut g, 5); // Start + 4 bets -> Trick(0)
        let s = encode(&g);
        for line in s.lines().filter(|l| l.starts_with("[Hand")) {
            // Extract content between the first and last '"' on the line.
            let after_first = line.split_once('"').map(|x| x.1).unwrap_or("");
            let inside = after_first.rsplit_once('"').map(|x| x.0).unwrap_or("");
            let cards: Vec<Card> = inside
                .split_whitespace()
                .map(|tok| super::super::format::parse_card(tok).unwrap())
                .collect();
            let mut sorted = cards.clone();
            sorted.sort();
            assert_eq!(cards, sorted, "hand not sorted: {}", line);
        }
    }

    #[test]
    fn aborted_from_not_started_emits_no_rounds() {
        let mut g = Game::new(
            fixed_uuid(1),
            [
                fixed_uuid(10),
                fixed_uuid(11),
                fixed_uuid(12),
                fixed_uuid(13),
            ],
            500,
            None,
        );
        g.set_state(State::Aborted);
        let s = encode(&g);
        assert!(s.contains("[Termination \"Aborted\"]\n"));
        assert!(
            !s.contains("[Round "),
            "no rounds should be emitted, got:\n{}",
            s
        );
    }
}
