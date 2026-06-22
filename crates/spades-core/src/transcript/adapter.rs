//! Adapter between the spades engine and the game-agnostic `trick_notation`
//! model. Ported from the retired Spades Transcript Format (STF) encoder/replayer;
//! only the serialization target changed (trick-notation events instead of STF
//! text). The per-round algorithms — dealt-hand reconstruction, per-round bets,
//! leader-tracked trick ordering, and the replay drive loop — are preserved.

use trick_notation::{Card as TnCard, DealtHand, Deck, Event, Meta, Model};

use crate::cards::{Card, Rank, Suit, get_trick_winner};
use crate::{Game, GameTransition, State, TimerConfig};

use super::ReplayError;

/// Seat symbols in seat order (seat index 0..4). Matches `meta.seats`.
const SEATS: [&str; 4] = ["N", "E", "S", "W"];

// ---------------------------------------------------------------------------
// Card <-> trick_notation::Card mapping
// ---------------------------------------------------------------------------

fn rank_sym(r: Rank) -> &'static str {
    match r {
        Rank::Two => "2",
        Rank::Three => "3",
        Rank::Four => "4",
        Rank::Five => "5",
        Rank::Six => "6",
        Rank::Seven => "7",
        Rank::Eight => "8",
        Rank::Nine => "9",
        Rank::Ten => "T",
        Rank::Jack => "J",
        Rank::Queen => "Q",
        Rank::King => "K",
        Rank::Ace => "A",
    }
}

fn suit_sym(s: Suit) -> &'static str {
    match s {
        Suit::Club => "C",
        Suit::Diamond => "D",
        Suit::Heart => "H",
        Suit::Spade => "S",
    }
}

fn card_to_tn(c: Card) -> TnCard {
    TnCard::Suited {
        suit: suit_sym(c.suit).to_string(),
        rank: rank_sym(c.rank).to_string(),
    }
}

fn rank_from_sym(s: &str) -> Option<Rank> {
    Some(match s {
        "2" => Rank::Two,
        "3" => Rank::Three,
        "4" => Rank::Four,
        "5" => Rank::Five,
        "6" => Rank::Six,
        "7" => Rank::Seven,
        "8" => Rank::Eight,
        "9" => Rank::Nine,
        "T" => Rank::Ten,
        "J" => Rank::Jack,
        "Q" => Rank::Queen,
        "K" => Rank::King,
        "A" => Rank::Ace,
        _ => return None,
    })
}

fn suit_from_sym(s: &str) -> Option<Suit> {
    Some(match s {
        "C" => Suit::Club,
        "D" => Suit::Diamond,
        "H" => Suit::Heart,
        "S" => Suit::Spade,
        _ => return None,
    })
}

/// Map a trick-notation card to a spades `Card`. A `Special` card or an unknown
/// suit/rank symbol can never occur for a spades game; it surfaces as a replay
/// error rather than being silently dropped.
fn tn_to_card(c: &TnCard) -> Result<Card, ReplayError> {
    match c {
        TnCard::Suited { suit, rank } => {
            let suit = suit_from_sym(suit).ok_or_else(|| ReplayError::BadCard {
                token: trick_notation::format_card(c),
            })?;
            let rank = rank_from_sym(rank).ok_or_else(|| ReplayError::BadCard {
                token: trick_notation::format_card(c),
            })?;
            Ok(Card { rank, suit })
        }
        TnCard::Special { .. } => Err(ReplayError::BadCard {
            token: trick_notation::format_card(c),
        }),
    }
}

// ===========================================================================
// game_to_model
// ===========================================================================

/// Build a rule-agnostic `trick_notation::Model` from a spades `Game`.
///
/// Total: every valid `Game` (any state) maps to a valid `Model`. Deterministic:
/// the same game state always produces the same model.
pub fn game_to_model(g: &Game) -> Model {
    Model {
        meta: build_meta(g),
        deck: Deck::french52(),
        events: build_events(g),
    }
}

fn build_meta(g: &Game) -> Meta {
    let names = g.get_player_names();

    let mut extra: Vec<(String, String)> = Vec::new();
    extra.push(("GameId".to_string(), g.get_id().to_string()));
    extra.push(("MaxPoints".to_string(), g.get_max_points().to_string()));
    for (i, (id, _)) in names.iter().enumerate() {
        extra.push((format!("Player{i}"), id.to_string()));
    }
    if let Some(t) = g.get_timer_config() {
        extra.push((
            "Timer".to_string(),
            format!("{}+{}", t.initial_time_secs, t.increment_secs),
        ));
    }
    let termination = match g.get_state() {
        State::Completed => "Completed",
        State::Aborted => "Aborted",
        _ => "InProgress",
    };
    extra.push(("Termination".to_string(), termination.to_string()));
    let result = match g.get_state() {
        State::Completed | State::Aborted => {
            let a = g.get_team_a_score().unwrap_or(0);
            let b = g.get_team_b_score().unwrap_or(0);
            format!("{a} {b}")
        }
        _ => "*".to_string(),
    };
    extra.push(("Result".to_string(), result));

    Meta {
        version: 1,
        game_hint: Some("spades".to_string()),
        seats: SEATS.iter().map(|s| s.to_string()).collect(),
        dealer: None,
        players: names
            .iter()
            .map(|(_, name)| name.map(|n| n.to_string()))
            .collect(),
        partnerships: Some(vec![
            vec!["N".to_string(), "S".to_string()],
            vec!["E".to_string(), "W".to_string()],
        ]),
        caps: vec![],
        extra,
    }
}

fn build_events(g: &Game) -> Vec<Event> {
    let num_rounds = num_rounds_to_emit(g);
    let mut events = Vec::new();
    for r in 0..num_rounds {
        // Deal.
        let hands = dealt_hands_for_round(g, r);
        let deal_hands = SEATS
            .iter()
            .zip(hands.iter())
            .map(|(seat, hand)| DealtHand {
                target: seat.to_string(),
                cards: hand.iter().map(|c| card_to_tn(*c)).collect(),
            })
            .collect();
        events.push(Event::Deal { hands: deal_hands });

        // Call (bets) — only if at least one bet was placed this round.
        let bets = bets_for_round(g, r);
        if !bets.is_empty() {
            events.push(Event::Call {
                start: SEATS[0].to_string(),
                values: bets.iter().map(|b| b.to_string()).collect(),
            });
        }

        // Plays — one per trick, in play order, leader recorded per trick.
        for (lead_index, trick) in tricks_for_round(g, r) {
            events.push(Event::Play {
                leader: SEATS[lead_index].to_string(),
                cards: trick.iter().map(|c| card_to_tn(*c)).collect(),
            });
        }
    }
    events
}

/// Number of round blocks to emit (ported verbatim from STF `encode.rs`).
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

/// Reconstruct the dealt hand per seat at the start of round `round_idx`.
/// Ported from STF `encode.rs`.
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
    // hand. For past completed rounds the engine has already dealt the next
    // round's cards into players' hands, so we must NOT pull from current hand.
    let is_current_round = match g.get_state() {
        State::Betting(_) | State::Trick(_) => g.get_round_index() == round_idx,
        State::Aborted => g.get_round_index() == round_idx,
        _ => false,
    };
    if is_current_round {
        let names = g.get_player_names();
        for (seat, hand) in hands.iter_mut().enumerate() {
            let pid = names[seat].0;
            if let Ok(remaining) = g.get_hand_by_player_id(pid) {
                hand.extend(remaining.iter().copied());
            }
        }
    }

    for h in &mut hands {
        h.sort();
    }
    hands
}

/// Bets to emit for round `round_idx`. May be 0..=4 entries. Ported from STF.
fn bets_for_round(g: &Game, round_idx: usize) -> Vec<i32> {
    let all = g.get_all_bets();
    let row = all.get(round_idx).copied().unwrap_or([0; 4]);
    let count = match g.get_state() {
        State::Betting(k) if g.get_round_index() == round_idx => *k,
        State::Aborted if g.get_round_index() == round_idx && g.is_in_betting_stage() => {
            // Cannot recover k from an Aborted-betting state precisely. Emit
            // all 4: over-reporting surfaces as a replay error later if the
            // trailing entries weren't actually placed. (Same lossiness the
            // STF encoder had — see module docs.)
            4
        }
        _ => 4,
    };
    row[..count].to_vec()
}

/// Tricks for round `round_idx`, each paired with its 0-based leader seat index.
/// Cards are returned in play order. Ported from STF `encode.rs` (leader is now
/// surfaced rather than dropped, because trick-notation records it explicitly).
fn tricks_for_round(g: &Game, round_idx: usize) -> Vec<(usize, Vec<Card>)> {
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
        let this_lead = lead;
        if count == 4 {
            let by_seat: [Card; 4] = [
                trick[0].unwrap(),
                trick[1].unwrap(),
                trick[2].unwrap(),
                trick[3].unwrap(),
            ];
            lead = get_trick_winner(lead, &by_seat);
        }
        out.push((this_lead, play_order));
    }
    out
}

// ===========================================================================
// model_to_game
// ===========================================================================

/// A round's data extracted from the event stream: declared hands (seat order),
/// bets (in `start` order, which is always N for spades), and tricks (play order
/// with the leader's seat index).
struct ParsedRound {
    hands: [Vec<Card>; 4],
    bets: Vec<i32>,
    tricks: Vec<(usize, Vec<Card>)>,
}

/// Drive a `trick_notation::Model` back into a fresh spades `Game`. Ported from
/// STF `replay.rs`; reads identity/config from `meta.extra` and groups the event
/// stream into rounds by `Deal` boundaries.
pub fn model_to_game(model: &Model) -> Result<Game, ReplayError> {
    let termination = get_extra(model, "Termination")
        .map(|s| s.as_str())
        .unwrap_or("InProgress")
        .to_string();
    let declared_result = parse_result(model)?;

    let mut game = build_game_from_meta(model)?;
    let rounds = parse_rounds(model)?;

    if rounds.is_empty() {
        finalize(&mut game, &termination, declared_result)?;
        return Ok(game);
    }

    // Game::play(Start) only fails when not in NotStarted; we just constructed a
    // fresh game, so any error here is a bug — panic rather than synthesize a
    // phantom Transition error.
    game.play(GameTransition::Start)
        .expect("freshly-constructed Game must accept Start");

    replay_rounds(&mut game, &rounds)?;

    finalize(&mut game, &termination, declared_result)?;
    Ok(game)
}

/// Cumulative `[team_a, team_b]` score after each fully-played round, computed by
/// replaying the model through the engine and snapshotting at round boundaries.
pub fn round_summaries(model: &Model) -> Result<Vec<[i32; 2]>, ReplayError> {
    let rounds = parse_rounds(model)?;

    if rounds.is_empty() {
        return Ok(Vec::new());
    }

    let mut game = build_game_from_meta(model)?;
    game.play(GameTransition::Start)
        .expect("freshly-constructed Game must accept Start");
    game.override_hands(rounds[0].hands.clone());

    let mut summaries: Vec<[i32; 2]> = Vec::new();

    for (r_idx, round) in rounds.iter().enumerate() {
        apply_bets(&mut game, &round.bets, r_idx)?;

        if round.bets.len() < 4 {
            if !round.tricks.is_empty() {
                return Err(ReplayError::InconsistentBetCount {
                    round: r_idx,
                    found: round.bets.len(),
                });
            }
            break;
        }

        apply_tricks(&mut game, &round.tricks, r_idx)?;

        // Only snapshot fully-played rounds (all 13 tricks completed). An
        // in-progress or aborted mid-trick round won't have 13 tricks and
        // hasn't been scored by the engine yet.
        if round.tricks.len() == 13 {
            summaries.push([
                game.get_team_a_score().unwrap_or(0),
                game.get_team_b_score().unwrap_or(0),
            ]);
        }

        let next = r_idx + 1;
        if next < rounds.len() {
            game.override_hands(rounds[next].hands.clone());
        }
    }

    Ok(summaries)
}

/// Construct a fresh `Game` from the model's `meta.extra` identity and config
/// fields, including player names.
fn build_game_from_meta(model: &Model) -> Result<Game, ReplayError> {
    let game_id = get_uuid(model, "GameId")?;
    let max_points = get_i32(model, "MaxPoints")?;
    let player_ids = [
        get_uuid(model, "Player0")?,
        get_uuid(model, "Player1")?,
        get_uuid(model, "Player2")?,
        get_uuid(model, "Player3")?,
    ];
    let timer = parse_timer(model)?;

    let mut game = Game::new(game_id, player_ids, max_points, timer);

    // Player names come from meta.players (seat order). Defensive against a
    // shorter/empty list (the text format omits [Players] entirely when no
    // seat is named).
    for (seat, name) in model.meta.players.iter().enumerate() {
        if seat >= 4 {
            break;
        }
        if let Some(n) = name {
            let _ = game.set_player_name(player_ids[seat], Some(n.clone()));
        }
    }

    Ok(game)
}

/// Drive the game through the parsed rounds sequentially, including hands
/// overrides between rounds. Does not call `Start` or `finalize`.
fn replay_rounds(game: &mut Game, rounds: &[ParsedRound]) -> Result<(), ReplayError> {
    game.override_hands(rounds[0].hands.clone());

    for (r_idx, round) in rounds.iter().enumerate() {
        apply_bets(game, &round.bets, r_idx)?;

        if round.bets.len() < 4 {
            if !round.tricks.is_empty() {
                return Err(ReplayError::InconsistentBetCount {
                    round: r_idx,
                    found: round.bets.len(),
                });
            }
            break;
        }

        apply_tricks(game, &round.tricks, r_idx)?;

        let next = r_idx + 1;
        if next < rounds.len() {
            game.override_hands(rounds[next].hands.clone());
        }
    }

    Ok(())
}

/// Apply a round's bets to `game`. Returns an error if any bet is rejected.
fn apply_bets(game: &mut Game, bets: &[i32], round: usize) -> Result<(), ReplayError> {
    for (i, &b) in bets.iter().enumerate() {
        game.play(GameTransition::Bet(b))
            .map_err(|e| ReplayError::Transition {
                round,
                trick: None,
                seat: i,
                err: e,
            })?;
    }
    Ok(())
}

/// Apply a round's tricks to `game`. Returns an error if any card play is rejected.
fn apply_tricks(
    game: &mut Game,
    tricks: &[(usize, Vec<Card>)],
    round: usize,
) -> Result<(), ReplayError> {
    for (t_idx, (leader, trick)) in tricks.iter().enumerate() {
        for (i, card) in trick.iter().enumerate() {
            let seat = (leader + i) % 4;
            game.play(GameTransition::Card(*card))
                .map_err(|e| ReplayError::Transition {
                    round,
                    trick: Some(t_idx),
                    seat,
                    err: e,
                })?;
        }
    }
    Ok(())
}

/// Group the event stream into rounds. Each round starts with a `Deal`. A `Call`
/// supplies bets; each `Play` supplies one trick (with its leader's seat index).
fn parse_rounds(model: &Model) -> Result<Vec<ParsedRound>, ReplayError> {
    let mut rounds: Vec<ParsedRound> = Vec::new();
    for event in &model.events {
        match event {
            Event::Deal { hands } => {
                rounds.push(ParsedRound {
                    hands: parse_deal_hands(hands)?,
                    bets: Vec::new(),
                    tricks: Vec::new(),
                });
            }
            Event::Call { values, .. } => {
                let round = rounds.last_mut().ok_or(ReplayError::EventBeforeDeal)?;
                let mut bets = Vec::with_capacity(values.len());
                for v in values {
                    bets.push(
                        v.parse::<i32>()
                            .map_err(|_| ReplayError::BadBet { value: v.clone() })?,
                    );
                }
                round.bets = bets;
            }
            Event::Play { leader, cards } => {
                let round = rounds.last_mut().ok_or(ReplayError::EventBeforeDeal)?;
                let lead_index = seat_index(leader)?;
                let mut trick = Vec::with_capacity(cards.len());
                for c in cards {
                    trick.push(tn_to_card(c)?);
                }
                round.tricks.push((lead_index, trick));
            }
            // Exchange / Reveal are not part of spades.
            Event::Exchange { .. } | Event::Reveal { .. } => {
                return Err(ReplayError::UnsupportedEvent);
            }
        }
    }
    Ok(rounds)
}

/// Map a `Deal`'s `DealtHand` entries to a seat-ordered `[Vec<Card>; 4]`.
fn parse_deal_hands(hands: &[DealtHand]) -> Result<[Vec<Card>; 4], ReplayError> {
    let mut out: [Vec<Card>; 4] = Default::default();
    for h in hands {
        let seat = seat_index(&h.target)?;
        for c in &h.cards {
            out[seat].push(tn_to_card(c)?);
        }
    }
    for h in &mut out {
        h.sort();
    }
    Ok(out)
}

fn seat_index(seat: &str) -> Result<usize, ReplayError> {
    SEATS
        .iter()
        .position(|s| *s == seat)
        .ok_or_else(|| ReplayError::BadSeat {
            seat: seat.to_string(),
        })
}

fn finalize(
    game: &mut Game,
    termination: &str,
    declared_result: Option<(i32, i32)>,
) -> Result<(), ReplayError> {
    if termination == "Aborted" && *game.get_state() != State::Completed {
        game.set_state(State::Aborted);
    }
    let actual = match game.get_state() {
        State::Completed => "Completed",
        State::Aborted => "Aborted",
        _ => "InProgress",
    };
    if actual != termination {
        return Err(ReplayError::TerminationMismatch {
            declared: termination.to_string(),
            actual: actual.to_string(),
        });
    }
    if let Some(declared) = declared_result {
        let a = game.get_team_a_score().unwrap_or(0);
        let b = game.get_team_b_score().unwrap_or(0);
        if (a, b) != declared {
            return Err(ReplayError::ResultMismatch {
                declared,
                actual: (a, b),
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// meta.extra accessors
// ---------------------------------------------------------------------------

fn get_extra<'a>(model: &'a Model, key: &str) -> Option<&'a String> {
    model
        .meta
        .extra
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

fn get_uuid(model: &Model, key: &'static str) -> Result<uuid::Uuid, ReplayError> {
    let v = get_extra(model, key).ok_or(ReplayError::MissingMeta { key })?;
    uuid::Uuid::parse_str(v).map_err(|_| ReplayError::BadMeta {
        key,
        value: v.clone(),
    })
}

fn get_i32(model: &Model, key: &'static str) -> Result<i32, ReplayError> {
    let v = get_extra(model, key).ok_or(ReplayError::MissingMeta { key })?;
    v.parse::<i32>().map_err(|_| ReplayError::BadMeta {
        key,
        value: v.clone(),
    })
}

/// Parse the `Timer` extra, formatted `"<initial>+<increment>"`. Absent → `None`.
fn parse_timer(model: &Model) -> Result<Option<TimerConfig>, ReplayError> {
    let Some(v) = get_extra(model, "Timer") else {
        return Ok(None);
    };
    let (init, inc) = v.split_once('+').ok_or_else(|| ReplayError::BadMeta {
        key: "Timer",
        value: v.clone(),
    })?;
    let initial_time_secs = init.parse::<u64>().map_err(|_| ReplayError::BadMeta {
        key: "Timer",
        value: v.clone(),
    })?;
    let increment_secs = inc.parse::<u64>().map_err(|_| ReplayError::BadMeta {
        key: "Timer",
        value: v.clone(),
    })?;
    Ok(Some(TimerConfig {
        initial_time_secs,
        increment_secs,
    }))
}

/// Parse the `Result` extra: `"*"` → `None`, `"<a> <b>"` → `Some((a, b))`.
fn parse_result(model: &Model) -> Result<Option<(i32, i32)>, ReplayError> {
    let Some(v) = get_extra(model, "Result") else {
        return Ok(None);
    };
    if v == "*" {
        return Ok(None);
    }
    let parts: Vec<&str> = v.split_whitespace().collect();
    let bad = || ReplayError::BadMeta {
        key: "Result",
        value: v.clone(),
    };
    if parts.len() != 2 {
        return Err(bad());
    }
    let a = parts[0].parse::<i32>().map_err(|_| bad())?;
    let b = parts[1].parse::<i32>().map_err(|_| bad())?;
    Ok(Some((a, b)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Game, GameTransition, State};
    use uuid::Uuid;

    fn u(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    /// Build a completed game by always betting 3 and playing the first legal
    /// card. Uses max_points = 50 so only a few rounds are needed.
    fn played_completed_game() -> Game {
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 50, None);
        loop {
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
                State::Completed | State::Aborted => break,
            }
        }
        g
    }

    #[test]
    fn round_summaries_are_monotonic_in_round_count() {
        // A completed low-max-points game produces one cumulative pair per round,
        // and the final pair equals the game's final team scores.
        let g = played_completed_game();
        let model = game_to_model(&g);
        let sums = round_summaries(&model).expect("summaries");
        assert!(!sums.is_empty());
        let last = *sums.last().unwrap();
        assert_eq!(
            last,
            [g.get_team_a_score().unwrap(), g.get_team_b_score().unwrap()]
        );

        let n_rounds = model
            .events
            .iter()
            .filter(|e| matches!(e, Event::Deal { .. }))
            .count();
        assert_eq!(
            sums.len(),
            n_rounds,
            "one cumulative summary per dealt round"
        );
    }
}
