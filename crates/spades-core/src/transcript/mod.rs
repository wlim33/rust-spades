//! Game serialization via the game-agnostic [`trick_notation`] format.
//!
//! [`encode`] renders a [`Game`](crate::Game) as canonical trick-notation text;
//! [`decode`] parses that text back into a [`Model`]; [`replay`] drives a
//! `Model` back through the engine into a fresh `Game`. For a terminal or
//! mid-game `Game` `g`:
//!
//! ```text
//! encode(&replay(&decode(&encode(&g)).unwrap()).unwrap()) == encode(&g)
//! ```
//!
//! holds (see the property test). This replaces the bespoke Spades Transcript
//! Format (STF) with the shared `trick_notation` model.
//!
//! # Known limitations
//!
//! - **Aborted-mid-betting is lossy.** When a game is aborted while in the
//!   betting phase, the encoder emits all 4 bet slots (un-placed bets default
//!   to 0) because the placed-bet count cannot be recovered from an aborted
//!   betting state. Replay then treats them as 4 real bets, so the replayed
//!   game's state may not be observationally equal to the source for this
//!   specific case. Aborted from a trick or terminal state round-trips cleanly.
//!   (Same limitation as the retired STF format.)
//!
//! - **Player names with whitespace or `"` do not round-trip through the
//!   canonical TEXT format.** The `[Players …]` header value is unquoted,
//!   so whitespace splits tokens and quote characters corrupt the syntax.
//!   The JSON projection of the model (names as a string array in `meta.players`)
//!   preserves names faithfully. Escaped/quoted header values are a planned
//!   follow-up enhancement to the trick-notation text grammar.

use crate::Game;
use crate::result::TransitionError;

mod adapter;

pub use trick_notation::Model;

/// Serialize a `Game` to canonical trick-notation text.
///
/// Total — every valid `Game` (any state) produces valid text. Deterministic:
/// the same state always produces byte-equal output.
pub fn encode(game: &Game) -> String {
    trick_notation::to_text(&adapter::game_to_model(game))
}

/// Parse trick-notation text into a [`Model`]. Performs only syntactic
/// validation; legality of the encoded moves is checked by [`replay`].
pub fn decode(text: &str) -> Result<Model, DecodeError> {
    trick_notation::from_text(text).map_err(|e| DecodeError::Text(e.to_string()))
}

/// Drive a parsed [`Model`] back into a fresh `Game` via the engine, verifying
/// the declared termination and result against the replayed end state.
pub fn replay(model: &Model) -> Result<Game, ReplayError> {
    adapter::model_to_game(model)
}

/// Build the game-agnostic notation model directly from a `Game`. Unlike
/// `decode(&encode(game))`, this preserves player names verbatim (the canonical
/// text format is lossy for names with whitespace/quotes — see Known limitations).
pub fn to_model(game: &Game) -> Model {
    adapter::game_to_model(game)
}

/// Failure parsing trick-notation text into a [`Model`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// The underlying `trick_notation` parser rejected the text.
    #[error("malformed transcript: {0}")]
    Text(String),
}

/// Failure replaying a [`Model`] back into a `Game`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplayError {
    /// `Game::play` rejected a transition synthesized from the model.
    #[error("illegal transition at round {round} trick {trick:?} seat {seat}: {err}")]
    Transition {
        round: usize,
        trick: Option<usize>,
        seat: usize,
        #[source]
        err: TransitionError,
    },
    /// Declared `Termination` doesn't match the state the replayed game ended in.
    #[error("termination mismatch: declared {declared}, replayed {actual}")]
    TerminationMismatch { declared: String, actual: String },
    /// Declared `Result` doesn't match replayed cumulative scores.
    #[error("result mismatch: declared {declared:?}, replayed {actual:?}")]
    ResultMismatch {
        declared: (i32, i32),
        actual: (i32, i32),
    },
    /// A round had a bet count not matching its play state (e.g. <4 bets but
    /// trick events present).
    #[error("inconsistent bet count in round {round}: found {found}")]
    InconsistentBetCount { round: usize, found: usize },
    /// A required `meta.extra` key was absent.
    #[error("missing required meta key {key:?}")]
    MissingMeta { key: &'static str },
    /// A `meta.extra` value failed to parse.
    #[error("invalid meta value for {key:?}: {value:?}")]
    BadMeta { key: &'static str, value: String },
    /// A card token could not be mapped to a spades card.
    #[error("invalid card token {token:?}")]
    BadCard { token: String },
    /// A seat symbol was not one of the four spades seats.
    #[error("invalid seat symbol {seat:?}")]
    BadSeat { seat: String },
    /// A bet value in a `Call` event was not an integer.
    #[error("invalid bet value {value:?}")]
    BadBet { value: String },
    /// A `Call` or `Play` event appeared before any `Deal`.
    #[error("event before any deal")]
    EventBeforeDeal,
    /// An event type not used by spades (Exchange/Reveal) appeared.
    #[error("unsupported event for spades")]
    UnsupportedEvent,
}

#[cfg(test)]
mod adapter_error_tests {
    //! Tests for error paths in `model_to_game` (adapter meta accessors).
    //! Build a valid base model via `encode`→`decode`, then mutate `meta.extra`
    //! to exercise each error branch.
    use super::*;

    fn base_model() -> Model {
        use crate::Game;
        use uuid::Uuid;
        let game_id = Uuid::from_bytes([1u8; 16]);
        let players = [
            Uuid::from_bytes([10u8; 16]),
            Uuid::from_bytes([11u8; 16]),
            Uuid::from_bytes([12u8; 16]),
            Uuid::from_bytes([13u8; 16]),
        ];
        let g = Game::new(game_id, players, 500, None);
        // NotStarted game encodes without events.
        decode(&encode(&g)).expect("base model decode")
    }

    /// Return a model with at least one Deal event (from a started-but-not-bet game).
    fn started_model() -> Model {
        use crate::{Game, GameTransition};
        use uuid::Uuid;
        let game_id = Uuid::from_bytes([2u8; 16]);
        let players = [
            Uuid::from_bytes([20u8; 16]),
            Uuid::from_bytes([21u8; 16]),
            Uuid::from_bytes([22u8; 16]),
            Uuid::from_bytes([23u8; 16]),
        ];
        let mut g = Game::new(game_id, players, 500, None);
        g.play(GameTransition::Start).unwrap();
        decode(&encode(&g)).expect("started model decode")
    }

    fn set_extra(model: &mut Model, key: &str, value: &str) {
        for (k, v) in model.meta.extra.iter_mut() {
            if k == key {
                *v = value.to_string();
                return;
            }
        }
        model.meta.extra.push((key.to_string(), value.to_string()));
    }

    fn remove_extra(model: &mut Model, key: &str) {
        model.meta.extra.retain(|(k, _)| k != key);
    }

    // --- Timer error paths --------------------------------------------------

    #[test]
    fn bad_timer_no_plus_separator() {
        let mut model = base_model();
        set_extra(&mut model, "Timer", "abc");
        assert!(
            matches!(
                replay(&model),
                Err(ReplayError::BadMeta { key: "Timer", .. })
            ),
            "expected BadMeta for Timer without '+'"
        );
    }

    #[test]
    fn bad_timer_non_numeric_initial() {
        let mut model = base_model();
        set_extra(&mut model, "Timer", "abc+5");
        assert!(
            matches!(
                replay(&model),
                Err(ReplayError::BadMeta { key: "Timer", .. })
            ),
            "expected BadMeta for Timer with non-numeric initial"
        );
    }

    #[test]
    fn bad_timer_non_numeric_increment() {
        let mut model = base_model();
        set_extra(&mut model, "Timer", "300+xyz");
        assert!(
            matches!(
                replay(&model),
                Err(ReplayError::BadMeta { key: "Timer", .. })
            ),
            "expected BadMeta for Timer with non-numeric increment"
        );
    }

    // --- Result error paths -------------------------------------------------

    #[test]
    fn bad_result_wrong_token_count() {
        let mut model = base_model();
        set_extra(&mut model, "Result", "100");
        assert!(
            matches!(
                replay(&model),
                Err(ReplayError::BadMeta { key: "Result", .. })
            ),
            "expected BadMeta for Result with only one token"
        );
    }

    #[test]
    fn bad_result_non_numeric() {
        let mut model = base_model();
        set_extra(&mut model, "Result", "abc def");
        assert!(
            matches!(
                replay(&model),
                Err(ReplayError::BadMeta { key: "Result", .. })
            ),
            "expected BadMeta for Result with non-numeric tokens"
        );
    }

    // --- UUID error paths ---------------------------------------------------

    #[test]
    fn bad_game_id_uuid() {
        let mut model = base_model();
        set_extra(&mut model, "GameId", "not-a-uuid");
        assert!(
            matches!(
                replay(&model),
                Err(ReplayError::BadMeta { key: "GameId", .. })
            ),
            "expected BadMeta for malformed GameId"
        );
    }

    #[test]
    fn missing_game_id() {
        let mut model = base_model();
        remove_extra(&mut model, "GameId");
        assert!(
            matches!(
                replay(&model),
                Err(ReplayError::MissingMeta { key: "GameId" })
            ),
            "expected MissingMeta for absent GameId"
        );
    }

    #[test]
    fn bad_player0_uuid() {
        let mut model = base_model();
        set_extra(&mut model, "Player0", "garbage");
        assert!(
            matches!(
                replay(&model),
                Err(ReplayError::BadMeta { key: "Player0", .. })
            ),
            "expected BadMeta for malformed Player0"
        );
    }

    #[test]
    fn bad_max_points_non_numeric() {
        let mut model = base_model();
        set_extra(&mut model, "MaxPoints", "notanumber");
        assert!(
            matches!(
                replay(&model),
                Err(ReplayError::BadMeta {
                    key: "MaxPoints",
                    ..
                })
            ),
            "expected BadMeta for non-numeric MaxPoints"
        );
    }

    // --- Structural error paths ---------------------------------------------

    #[test]
    fn unsupported_event_exchange_rejected() {
        use trick_notation::{Card as TnCard, Event};
        let mut model = base_model();
        model.events.push(Event::Exchange {
            from: "N".to_string(),
            to: "E".to_string(),
            cards: vec![TnCard::Suited {
                suit: "S".into(),
                rank: "A".into(),
            }],
        });
        assert!(
            matches!(replay(&model), Err(ReplayError::UnsupportedEvent)),
            "expected UnsupportedEvent for Exchange event"
        );
    }

    #[test]
    fn unsupported_event_reveal_rejected() {
        use trick_notation::{Card as TnCard, Event};
        let mut model = base_model();
        model.events.push(Event::Reveal {
            target: "N".to_string(),
            cards: vec![TnCard::Suited {
                suit: "S".into(),
                rank: "A".into(),
            }],
        });
        assert!(
            matches!(replay(&model), Err(ReplayError::UnsupportedEvent)),
            "expected UnsupportedEvent for Reveal event"
        );
    }

    #[test]
    fn call_event_before_deal_rejected() {
        use trick_notation::Event;
        let mut model = base_model();
        // Insert a Call before any Deal.
        model.events.insert(
            0,
            Event::Call {
                start: "N".to_string(),
                values: vec!["3".to_string()],
            },
        );
        assert!(
            matches!(replay(&model), Err(ReplayError::EventBeforeDeal)),
            "expected EventBeforeDeal for Call before any Deal"
        );
    }

    #[test]
    fn play_event_before_deal_rejected() {
        use trick_notation::{Card as TnCard, Event};
        let mut model = base_model();
        model.events.insert(
            0,
            Event::Play {
                leader: "N".to_string(),
                cards: vec![TnCard::Suited {
                    suit: "S".into(),
                    rank: "A".into(),
                }],
            },
        );
        assert!(
            matches!(replay(&model), Err(ReplayError::EventBeforeDeal)),
            "expected EventBeforeDeal for Play before any Deal"
        );
    }

    #[test]
    fn bad_seat_in_play_event() {
        use trick_notation::{Card as TnCard, Event};
        // Use a started model (has a Deal) so the Play gets past EventBeforeDeal.
        let mut m = started_model();
        // Append a Play with an invalid leader seat symbol.
        m.events.push(Event::Play {
            leader: "BADseat".to_string(),
            cards: vec![TnCard::Suited {
                suit: "S".into(),
                rank: "A".into(),
            }],
        });
        assert!(
            matches!(replay(&m), Err(ReplayError::BadSeat { .. })),
            "expected BadSeat for invalid seat symbol in Play"
        );
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;
    use crate::result::TransitionError;

    #[test]
    fn replay_error_display_includes_transition() {
        let err = ReplayError::Transition {
            round: 0,
            trick: None,
            seat: 0,
            err: TransitionError::NotStarted,
        };
        let s = err.to_string();
        assert!(s.contains("illegal transition"), "{s}");
        assert!(
            s.contains("Attempted to play a game not started yet"),
            "{s}"
        );
    }

    #[test]
    fn decode_error_display_is_descriptive() {
        let e = DecodeError::Text("boom".into());
        assert_eq!(e.to_string(), "malformed transcript: boom");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Game, GameTransition, State, TimerConfig};
    use trick_notation::Event;
    use uuid::Uuid;

    fn u(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn new_game(max_points: i32, timer: Option<TimerConfig>) -> Game {
        Game::new(u(1), [u(10), u(11), u(12), u(13)], max_points, timer)
    }

    /// Drive a Game by always taking the first legal card / Bet(3) / Start, up
    /// to `transitions` steps (stops early at a terminal state).
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

    fn round_trip(g: &Game) -> Game {
        let text = encode(g);
        let model = decode(&text).expect("decode");
        replay(&model).expect("replay")
    }

    // --- meta / structural facts -------------------------------------------

    #[test]
    fn meta_has_spades_shape() {
        let g = new_game(500, None);
        let model = decode(&encode(&g)).unwrap();
        assert_eq!(model.meta.game_hint.as_deref(), Some("spades"));
        assert_eq!(model.meta.seats, vec!["N", "E", "S", "W"]);
        assert_eq!(
            model.meta.partnerships,
            Some(vec![
                vec!["N".to_string(), "S".to_string()],
                vec!["E".to_string(), "W".to_string()],
            ])
        );
        assert_eq!(model.deck, trick_notation::Deck::french52());
    }

    #[test]
    fn not_started_has_no_round_events() {
        let g = new_game(500, None);
        let model = decode(&encode(&g)).unwrap();
        assert!(model.events.is_empty(), "no deals before Start");
    }

    // --- mid-first-bet (partial Call) --------------------------------------

    #[test]
    fn mid_first_bet_emits_partial_call() {
        let mut g = new_game(500, None);
        play_first_legal(&mut g, 1); // Start
        play_first_legal(&mut g, 2); // 2 bets

        let model = decode(&encode(&g)).unwrap();
        // One Deal, then a Call with exactly the two placed bets, no Plays.
        let deals = model
            .events
            .iter()
            .filter(|e| matches!(e, Event::Deal { .. }))
            .count();
        assert_eq!(deals, 1);
        let call = model
            .events
            .iter()
            .find_map(|e| match e {
                Event::Call { values, .. } => Some(values.clone()),
                _ => None,
            })
            .expect("a Call event");
        assert_eq!(call, vec!["3".to_string(), "3".to_string()]);
        assert!(
            !model.events.iter().any(|e| matches!(e, Event::Play { .. })),
            "no plays should be emitted mid-betting"
        );

        // Round-trips to the same state.
        let r = round_trip(&g);
        assert_eq!(r.get_state(), g.get_state());
        assert_eq!(encode(&r), encode(&g));
    }

    // --- completed short game ----------------------------------------------

    #[test]
    fn completed_short_game_round_trips() {
        let mut g = new_game(50, None);
        play_first_legal(&mut g, 10_000);
        assert_eq!(*g.get_state(), State::Completed);

        let model = decode(&encode(&g)).unwrap();
        // Termination/Result are present and terminal.
        let term = model
            .meta
            .extra
            .iter()
            .find(|(k, _)| k == "Termination")
            .map(|(_, v)| v.clone());
        assert_eq!(term.as_deref(), Some("Completed"));
        let result = model
            .meta
            .extra
            .iter()
            .find(|(k, _)| k == "Result")
            .map(|(_, v)| v.clone())
            .unwrap();
        assert_ne!(result, "*");
        assert!(result.contains(' '));
        // At least one deal happened.
        assert!(model.events.iter().any(|e| matches!(e, Event::Deal { .. })));

        let r = round_trip(&g);
        assert_eq!(r.get_id(), g.get_id());
        assert_eq!(r.get_state(), g.get_state());
        assert_eq!(
            r.get_team_a_score().unwrap_or(0),
            g.get_team_a_score().unwrap_or(0)
        );
        assert_eq!(
            r.get_team_b_score().unwrap_or(0),
            g.get_team_b_score().unwrap_or(0)
        );
        assert_eq!(encode(&r), encode(&g));
    }

    // --- aborted from NotStarted -------------------------------------------

    #[test]
    fn aborted_from_not_started_emits_no_round_events() {
        let mut g = new_game(500, None);
        g.set_state(State::Aborted);
        let model = decode(&encode(&g)).unwrap();
        assert!(
            model.events.is_empty(),
            "no round events for aborted-from-NotStarted"
        );
        let term = model
            .meta
            .extra
            .iter()
            .find(|(k, _)| k == "Termination")
            .map(|(_, v)| v.clone());
        assert_eq!(term.as_deref(), Some("Aborted"));

        let r = round_trip(&g);
        assert_eq!(*r.get_state(), State::Aborted);
        assert_eq!(encode(&r), encode(&g));
    }

    // --- timer + names round-trip ------------------------------------------

    #[test]
    fn timer_and_names_round_trip() {
        let mut g = new_game(
            300,
            Some(TimerConfig {
                initial_time_secs: 300,
                increment_secs: 5,
            }),
        );
        g.set_player_name(u(10), Some("Alice".into())).unwrap();
        g.set_player_name(u(12), Some("Carol".into())).unwrap();

        let r = round_trip(&g);
        let names = r.get_player_names();
        assert_eq!(names[0].1, Some("Alice"));
        assert_eq!(names[1].1, None);
        assert_eq!(names[2].1, Some("Carol"));
        assert_eq!(names[3].1, None);
        assert_eq!(
            r.get_timer_config()
                .map(|t| (t.initial_time_secs, t.increment_secs)),
            Some((300, 5))
        );
        assert_eq!(encode(&r), encode(&g));
    }

    // --- mid-trick round-trip ----------------------------------------------

    #[test]
    fn mid_trick_round_trips() {
        let mut g = new_game(500, None);
        play_first_legal(&mut g, 1);
        play_first_legal(&mut g, 4); // 4 bets -> Trick
        // Play 2 cards into the first trick.
        for _ in 0..2 {
            let legal = g.get_legal_cards().unwrap();
            g.play(GameTransition::Card(legal[0])).unwrap();
        }
        let r = round_trip(&g);
        assert_eq!(r.get_state(), g.get_state());
        assert_eq!(encode(&r), encode(&g));
    }

    // --- nil bid round-trip ------------------------------------------------

    #[test]
    fn nil_bid_round_trips() {
        let mut g = new_game(500, None);
        g.play(GameTransition::Start).unwrap();
        g.play(GameTransition::Bet(0)).unwrap();
        g.play(GameTransition::Bet(3)).unwrap();
        g.play(GameTransition::Bet(3)).unwrap();
        g.play(GameTransition::Bet(3)).unwrap();
        for _ in 0..13 {
            for _ in 0..4 {
                let legal = g.get_legal_cards().unwrap();
                g.play(GameTransition::Card(legal[0])).unwrap();
            }
        }
        let model = decode(&encode(&g)).unwrap();
        let call = model
            .events
            .iter()
            .find_map(|e| match e {
                Event::Call { values, .. } => Some(values.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(call[0], "0", "nil bid should serialize as 0");

        let r = round_trip(&g);
        assert_eq!(r.get_state(), g.get_state());
        assert_eq!(encode(&r), encode(&g));
    }

    // --- replay rejection paths --------------------------------------------

    #[test]
    fn replay_rejects_termination_mismatch() {
        let g = new_game(500, None);
        let mut model = decode(&encode(&g)).unwrap();
        for (k, v) in model.meta.extra.iter_mut() {
            if k == "Termination" {
                *v = "Completed".to_string();
            }
            if k == "Result" {
                *v = "100 50".to_string();
            }
        }
        assert!(matches!(
            replay(&model),
            Err(ReplayError::TerminationMismatch { .. })
        ));
    }

    #[test]
    fn replay_rejects_result_mismatch() {
        let mut g = new_game(50, None);
        play_first_legal(&mut g, 10_000);
        let mut model = decode(&encode(&g)).unwrap();
        for (k, v) in model.meta.extra.iter_mut() {
            if k == "Result" {
                // Bump team A's score so it can't match.
                let mut parts = v.split_whitespace();
                let a: i32 = parts.next().unwrap().parse().unwrap();
                let b: i32 = parts.next().unwrap().parse().unwrap();
                *v = format!("{} {}", a + 999, b);
            }
        }
        assert!(matches!(
            replay(&model),
            Err(ReplayError::ResultMismatch { .. })
        ));
    }

    #[test]
    fn replay_rejects_illegal_card() {
        let mut g = new_game(500, None);
        play_first_legal(&mut g, 1);
        play_first_legal(&mut g, 4);
        for _ in 0..4 {
            let legal = g.get_legal_cards().unwrap();
            g.play(GameTransition::Card(legal[0])).unwrap();
        }
        let mut model = decode(&encode(&g)).unwrap();
        // Replace the first card of the first Play with AS (very likely not in
        // seat 0's declared hand, and even if it is, the lead-suit/spades rules
        // make it illegal as a swap).
        for e in model.events.iter_mut() {
            if let Event::Play { cards, .. } = e {
                cards[0] = trick_notation::Card::Suited {
                    suit: "S".into(),
                    rank: "A".into(),
                };
                break;
            }
        }
        assert!(matches!(
            replay(&model),
            Err(ReplayError::Transition { .. })
        ));
    }

    #[test]
    fn replay_rejects_inconsistent_bet_count() {
        let mut g = new_game(500, None);
        play_first_legal(&mut g, 1);
        play_first_legal(&mut g, 4);
        let legal = g.get_legal_cards().unwrap();
        g.play(GameTransition::Card(legal[0])).unwrap();

        let mut model = decode(&encode(&g)).unwrap();
        // Truncate the Call to 2 bets while a Play exists.
        for e in model.events.iter_mut() {
            if let Event::Call { values, .. } = e {
                values.truncate(2);
            }
        }
        assert!(matches!(
            replay(&model),
            Err(ReplayError::InconsistentBetCount { round: 0, found: 2 })
        ));
    }

    #[test]
    fn replay_rejects_bet_in_trick_stage() {
        let mut g = new_game(500, None);
        play_first_legal(&mut g, 1);
        play_first_legal(&mut g, 4);
        let mut model = decode(&encode(&g)).unwrap();
        for e in model.events.iter_mut() {
            if let Event::Call { values, .. } = e {
                values.push("3".to_string()); // a 5th bet
            }
        }
        let err = replay(&model).expect_err("5th bet should be rejected");
        assert!(matches!(
            err,
            ReplayError::Transition {
                round: 0,
                trick: None,
                seat: 4,
                err: crate::TransitionError::BetInTrickStage,
            }
        ));
    }

    #[test]
    fn replay_rejects_bad_card_token() {
        let mut g = new_game(500, None);
        play_first_legal(&mut g, 1);
        play_first_legal(&mut g, 4);
        for _ in 0..4 {
            let legal = g.get_legal_cards().unwrap();
            g.play(GameTransition::Card(legal[0])).unwrap();
        }
        let mut model = decode(&encode(&g)).unwrap();
        for e in model.events.iter_mut() {
            if let Event::Play { cards, .. } = e {
                cards[0] = trick_notation::Card::Special {
                    name: "Joker".into(),
                };
                break;
            }
        }
        assert!(matches!(replay(&model), Err(ReplayError::BadCard { .. })));
    }

    #[test]
    fn replay_rejects_missing_meta() {
        let g = new_game(500, None);
        let mut model = decode(&encode(&g)).unwrap();
        model.meta.extra.retain(|(k, _)| k != "GameId");
        assert!(matches!(
            replay(&model),
            Err(ReplayError::MissingMeta { key: "GameId" })
        ));
    }

    #[test]
    fn names_with_spaces_are_a_documented_text_format_limitation() {
        let mut g = new_game(500, None);
        g.set_player_name(u(10), Some("Bo Jones".into())).unwrap();
        g.set_player_name(u(11), Some("Alice".into())).unwrap();

        let text = encode(&g);
        // Inspect the [Players ...] line to document current (lossy) behavior
        let players_line = text
            .lines()
            .find(|line| line.starts_with("[Players"))
            .expect("encoded text should have [Players line");

        // The text format produces [Players "Bo Jones Alice ? ?"] (space unescaped).
        // A parser splitting on whitespace cannot recover "Bo Jones" as a single name.
        assert_eq!(
            players_line, "[Players \"Bo Jones Alice ? ?\"]",
            "player names with spaces are not escaped in the text format"
        );

        // The text format round-trip is LOSSY for names with spaces.
        // Parsing [Players "Bo Jones Alice ? ?"] splits on whitespace.
        // Names are drawn sequentially as whitespace-delimited tokens → "Bo", "Jones", "Alice".
        let r = round_trip(&g);
        let names = r.get_player_names();
        assert_eq!(names[0].1, Some("Bo"), "first name token from 'Bo Jones'");
        assert_eq!(
            names[1].1,
            Some("Jones"),
            "second name token (from 'Bo Jones'); Alice shifts to next seat"
        );
    }

    #[test]
    fn to_model_matches_encode_decode_path_for_simple_game() {
        // to_model(game) must equal decode(encode(game)) when round-tripped:
        // This verifies that to_model produces a Model structurally equivalent
        // to the one recovered by parsing encode(game).
        // Use UUID-only player names (no set_player_name); the text format still
        // doesn't round-trip the meta.players field, which is a known limitation.
        let mut g = new_game(500, None);
        g.play(GameTransition::Start).unwrap();
        g.play(GameTransition::Bet(3)).unwrap();
        g.play(GameTransition::Bet(3)).unwrap();
        g.play(GameTransition::Bet(3)).unwrap();
        g.play(GameTransition::Bet(3)).unwrap();
        // Play a few legal cards into the first trick.
        for _ in 0..2 {
            let legal = g.get_legal_cards().unwrap();
            g.play(GameTransition::Card(legal[0])).unwrap();
        }

        // Verify: to_model(g) should structurally equal decode(encode(g)).
        // Both paths produce a Model; they should be equal, not just text-equal.
        let text = encode(&g);
        let mut direct = to_model(&g);
        let mut via_text = decode(&text).unwrap();
        // The text format is lossy for meta.players (it doesn't persist the player
        // name slots). Clear it from the direct path to match the text result.
        direct.meta.players.clear();
        via_text.meta.players.clear();
        // The text format lists cards in deal order, not sorted; normalize both
        // by sorting the hands in each Deal event so comparison is fair.
        for event in direct.events.iter_mut().chain(via_text.events.iter_mut()) {
            if let Event::Deal { hands } = event {
                for (_seat, cards) in hands.iter_mut() {
                    cards.sort_by(|a, b| {
                        let a_str = trick_notation::format_card(a);
                        let b_str = trick_notation::format_card(b);
                        a_str.cmp(&b_str)
                    });
                }
            }
        }
        assert_eq!(direct, via_text);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::{Game, GameTransition, State};
    use rand::rngs::StdRng;
    use rand::seq::IndexedRandom;
    use rand::{Rng, SeedableRng};
    use uuid::Uuid;

    fn play_full_random_game(seed: u64) -> Game {
        let mut rng = StdRng::seed_from_u64(seed);

        let mut id_bytes = [0u8; 16];
        id_bytes[..8].copy_from_slice(&seed.to_be_bytes());
        id_bytes[8..].copy_from_slice(&(!seed).to_be_bytes());
        let game_id = Uuid::from_bytes(id_bytes);

        let player_ids = [
            Uuid::from_bytes([1; 16]),
            Uuid::from_bytes([2; 16]),
            Uuid::from_bytes([3; 16]),
            Uuid::from_bytes([4; 16]),
        ];

        let mut g = Game::new(game_id, player_ids, 60, None);
        g.play(GameTransition::Start).unwrap();
        loop {
            match *g.get_state() {
                State::Completed | State::Aborted => return g,
                State::Betting(_) => {
                    let b = (rng.next_u32() % 4) as i32 + 1;
                    g.play(GameTransition::Bet(b)).unwrap();
                }
                State::Trick(_) => {
                    let legal = g.get_legal_cards().unwrap();
                    let card = *legal.choose(&mut rng).unwrap();
                    g.play(GameTransition::Card(card)).unwrap();
                }
                State::NotStarted => unreachable!(),
            }
        }
    }

    #[test]
    fn round_trip_is_idempotent_on_many_random_games() {
        for seed in 0..30u64 {
            let g = play_full_random_game(seed);
            let s1 = encode(&g);
            let model = decode(&s1).expect("decode");
            let replayed = replay(&model).expect("replay");
            let s2 = encode(&replayed);
            assert_eq!(s1, s2, "round trip differed for seed {seed}");
        }
    }
}
