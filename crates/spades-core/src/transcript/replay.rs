use crate::cards::{Card, get_trick_winner};
use crate::{Game, GameTransition, State};

use super::{ReplayError, Termination, Transcript};

/// Drive a parsed `Transcript` back into a fresh `Game` via the engine.
///
/// Replay constructs a new `Game` with the transcript's headers, then issues
/// `GameTransition::{Start, Bet, Card}` calls in order. Each round's declared
/// dealt hands are injected via `Game::override_hands` (because `Game::Start`
/// shuffles randomly). Any rule violation surfaces as `ReplayError::Transition`.
///
/// After replay, declared `Termination` and `Result` are verified against the
/// engine's actual end state; mismatches return `ReplayError::TerminationMismatch`
/// or `ReplayError::ResultMismatch`.
///
/// Aborted termination is applied via `Game::set_state(Aborted)` if the
/// transcript declares Aborted and the engine hasn't already reached Completed.
pub fn replay(t: &Transcript) -> Result<Game, ReplayError> {
    let mut game = Game::new(
        t.headers.game_id,
        t.headers.player_ids,
        t.headers.max_points,
        t.headers.timer,
    );
    for seat in 0..4 {
        if let Some(name) = &t.headers.names[seat] {
            let _ = game.set_player_name(t.headers.player_ids[seat], Some(name.clone()));
        }
    }

    if t.rounds.is_empty() {
        finalize(&mut game, t)?;
        return Ok(game);
    }

    // Game::play(Start) only fails when the game isn't in NotStarted, and
    // we just constructed a fresh one, so any error here is a bug — panic
    // rather than synthesize a phantom Transition error.
    game.play(GameTransition::Start)
        .expect("freshly-constructed Game must accept Start");

    // Seed the engine with the transcript's declared hands rather than the
    // randomly-dealt ones from Start.
    game.override_hands(t.rounds[0].hands.clone());

    for (r_idx, round) in t.rounds.iter().enumerate() {
        for (i, &b) in round.bets.iter().enumerate() {
            game.play(GameTransition::Bet(b))
                .map_err(|e| ReplayError::Transition {
                    round: r_idx,
                    trick: None,
                    seat: i,
                    err: e,
                })?;
        }

        if round.bets.len() < 4 {
            if !round.tricks.is_empty() {
                return Err(ReplayError::InconsistentBetCount {
                    round: r_idx,
                    found: round.bets.len(),
                });
            }
            break;
        }

        let mut lead = 0usize;
        for (t_idx, trick) in round.tricks.iter().enumerate() {
            for (i, &card) in trick.iter().enumerate() {
                let seat = (lead + i) % 4;
                game.play(GameTransition::Card(card))
                    .map_err(|e| ReplayError::Transition {
                        round: r_idx,
                        trick: Some(t_idx),
                        seat,
                        err: e,
                    })?;
            }
            if trick.len() == 4 {
                let mut by_seat = [Card {
                    rank: crate::cards::Rank::Two,
                    suit: crate::cards::Suit::Club,
                }; 4];
                for i in 0..4 {
                    by_seat[(lead + i) % 4] = trick[i];
                }
                lead = get_trick_winner(lead, &by_seat);
            }
        }

        // If a subsequent round exists, the engine just re-dealt randomly.
        // Override with the transcript's declared hands for the next round.
        let next = r_idx + 1;
        if next < t.rounds.len() {
            game.override_hands(t.rounds[next].hands.clone());
        }
    }

    finalize(&mut game, t)?;
    Ok(game)
}

fn finalize(game: &mut Game, t: &Transcript) -> Result<(), ReplayError> {
    if matches!(t.termination, Termination::Aborted) && *game.get_state() != State::Completed {
        game.set_state(State::Aborted);
    }
    let actual = match game.get_state() {
        State::Completed => Termination::Completed,
        State::Aborted => Termination::Aborted,
        _ => Termination::InProgress,
    };
    if actual != t.termination {
        return Err(ReplayError::TerminationMismatch {
            declared: t.termination,
            actual,
        });
    }
    if let Some(declared) = t.result {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{decode, encode};
    use uuid::Uuid;

    fn u(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn build_short_game() -> Game {
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 50, None);
        g.play(GameTransition::Start).unwrap();
        for _ in 0..4 {
            g.play(GameTransition::Bet(3)).unwrap();
        }
        for _ in 0..13 {
            for _ in 0..4 {
                let legal = g.get_legal_cards().unwrap();
                g.play(GameTransition::Card(legal[0])).unwrap();
            }
        }
        g
    }

    #[test]
    fn replay_one_round_round_trip() {
        let g = build_short_game();
        let encoded = encode(&g);
        let parsed = decode(&encoded).expect("decode");
        let replayed = replay(&parsed).expect("replay");

        assert_eq!(replayed.get_id(), g.get_id());
        assert_eq!(
            replayed.get_team_a_score().unwrap_or(0),
            g.get_team_a_score().unwrap_or(0)
        );
        assert_eq!(
            replayed.get_team_b_score().unwrap_or(0),
            g.get_team_b_score().unwrap_or(0)
        );
        assert_eq!(replayed.get_state(), g.get_state());
        let re_encoded = encode(&replayed);
        assert_eq!(encoded, re_encoded, "encoder idempotence");
    }

    #[test]
    fn replay_mid_betting() {
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 500, None);
        g.play(GameTransition::Start).unwrap();
        g.play(GameTransition::Bet(3)).unwrap();
        g.play(GameTransition::Bet(2)).unwrap();
        let encoded = encode(&g);
        let parsed = decode(&encoded).unwrap();
        let replayed = replay(&parsed).unwrap();
        assert_eq!(replayed.get_state(), g.get_state());
        assert_eq!(encode(&replayed), encoded);
    }

    #[test]
    fn replay_mid_trick() {
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 500, None);
        g.play(GameTransition::Start).unwrap();
        for _ in 0..4 {
            g.play(GameTransition::Bet(3)).unwrap();
        }
        for _ in 0..2 {
            let legal = g.get_legal_cards().unwrap();
            g.play(GameTransition::Card(legal[0])).unwrap();
        }
        let encoded = encode(&g);
        let parsed = decode(&encoded).unwrap();
        let replayed = replay(&parsed).unwrap();
        assert_eq!(replayed.get_state(), g.get_state());
        assert_eq!(encode(&replayed), encoded);
    }

    #[test]
    fn replay_aborted_from_betting() {
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 500, None);
        g.play(GameTransition::Start).unwrap();
        g.play(GameTransition::Bet(3)).unwrap();
        g.set_state(State::Aborted);
        let encoded = encode(&g);
        let parsed = decode(&encoded).unwrap();
        let replayed = replay(&parsed).unwrap();
        assert_eq!(replayed.get_state(), &State::Aborted);
    }

    #[test]
    fn replay_rejects_termination_mismatch() {
        let g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 500, None);
        let encoded = encode(&g)
            .replace("InProgress", "Completed")
            .replace("\"*\"", "\"100 50\"");
        let parsed = decode(&encoded).unwrap();
        assert!(matches!(
            replay(&parsed),
            Err(ReplayError::TerminationMismatch { .. })
        ));
    }

    #[test]
    fn replay_rejects_illegal_card() {
        // Use an in-progress game (mid-trick) so Result is "*" and always
        // decodes cleanly, regardless of score sign.
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 500, None);
        g.play(GameTransition::Start).unwrap();
        for _ in 0..4 {
            g.play(GameTransition::Bet(3)).unwrap();
        }
        // Play at least one complete trick so tricks[0] exists.
        for _ in 0..4 {
            let legal = g.get_legal_cards().unwrap();
            g.play(GameTransition::Card(legal[0])).unwrap();
        }
        let encoded = encode(&g);
        let mut parsed = decode(&encoded).unwrap();
        // Force the first card of round 0 trick 0 to AS. override_hands has
        // already set seat 0's hand to the (un-tampered) declared cards, so
        // the only way the play succeeds is if AS happens to be in that hand;
        // otherwise the engine rejects with Transition::CardNotInHand.
        parsed.rounds[0].tricks[0][0] = Card {
            rank: crate::cards::Rank::Ace,
            suit: crate::cards::Suit::Spade,
        };
        assert!(matches!(
            replay(&parsed),
            Err(ReplayError::Transition { .. })
        ));
    }

    #[test]
    fn round_trip_with_nil_bid_in_first_round() {
        // Seat 0 bids nil (Bet(0)). Plays 13 tricks (whoever the engine routes
        // to). Verify encode -> decode -> replay round-trips cleanly without
        // requiring the game to reach a terminal state. Captures the nil-bid
        // path that the property test can't sample (random nil play diverges
        // beyond max_points and the engine never terminates).
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 500, None);
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
        let encoded = encode(&g);
        // Sanity: the [Bets ...] line should contain a 0 in the first slot.
        assert!(
            encoded.contains("[Bets \"0 3 3 3\"]"),
            "expected nil bid in encoded bets line, got:\n{}",
            encoded
        );
        let parsed = decode(&encoded).unwrap();
        let replayed = replay(&parsed).unwrap();
        assert_eq!(replayed.get_state(), g.get_state());
        assert_eq!(
            encode(&replayed),
            encoded,
            "round-trip idempotence for nil-bid game"
        );
    }

    #[test]
    fn replay_rejects_inconsistent_bet_count() {
        // Round 0 has 2 bets but 1 trick line: replay should refuse rather
        // than silently replay 2 bets and then attempt to play cards while
        // still in the betting state.
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 500, None);
        g.play(GameTransition::Start).unwrap();
        for _ in 0..4 {
            g.play(GameTransition::Bet(3)).unwrap();
        }
        let legal = g.get_legal_cards().unwrap();
        g.play(GameTransition::Card(legal[0])).unwrap();

        let encoded = encode(&g);
        let mut parsed = decode(&encoded).unwrap();
        parsed.rounds[0].bets.pop();
        parsed.rounds[0].bets.pop();
        assert_eq!(parsed.rounds[0].bets.len(), 2);
        assert!(!parsed.rounds[0].tricks.is_empty());
        assert!(matches!(
            replay(&parsed),
            Err(ReplayError::InconsistentBetCount { round: 0, found: 2 })
        ));
    }

    #[test]
    fn replay_rejects_bet_in_trick_stage() {
        // Synthesize a Transcript with 5 bets in round 0: the engine will
        // refuse the 5th because it has already transitioned to the trick
        // stage, surfacing as ReplayError::Transition.
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 500, None);
        g.play(GameTransition::Start).unwrap();
        for _ in 0..4 {
            g.play(GameTransition::Bet(3)).unwrap();
        }
        let encoded = encode(&g);
        let mut parsed = decode(&encoded).unwrap();
        parsed.rounds[0].bets.push(3);

        let err = replay(&parsed).expect_err("replay should reject 5th bet");
        assert!(matches!(
            err,
            ReplayError::Transition {
                round: 0,
                trick: None,
                seat: 4,
                err: crate::TransitionError::BetInTrickStage
            }
        ));
    }

    #[test]
    fn replay_rejects_result_mismatch() {
        let g = build_short_game();
        let encoded = encode(&g);
        let mut parsed = decode(&encoded).unwrap();
        let bogus = (parsed.result.unwrap().0 + 999, parsed.result.unwrap().1);
        parsed.result = Some(bogus);
        assert!(matches!(
            replay(&parsed),
            Err(ReplayError::ResultMismatch { declared, .. }) if declared == bogus
        ));
    }

    #[test]
    fn replay_preserves_player_names() {
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 500, None);
        g.set_player_name(u(10), Some("Alice".into())).unwrap();
        g.set_player_name(u(12), Some("Carol \"Q\"".into()))
            .unwrap();
        let encoded = encode(&g);
        let parsed = decode(&encoded).unwrap();
        let replayed = replay(&parsed).unwrap();
        let names = replayed.get_player_names();
        assert_eq!(names[0].1, Some("Alice"));
        assert_eq!(names[1].1, None);
        assert_eq!(names[2].1, Some("Carol \"Q\""));
        assert_eq!(names[3].1, None);
    }

    /// Deal known hands to all four seats. Hand 0 holds clubs (2C..5C),
    /// diamonds (2D..5D), hearts (2H..4H), and spades (2S, 3S) — enough
    /// suit diversity to trigger every follow-suit rule under test.
    fn rig_game_with_fixed_hands() -> Game {
        let hand = |s: &str| -> Vec<Card> {
            s.split_whitespace()
                .map(|tok| super::super::format::parse_card(tok).unwrap())
                .collect()
        };
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 500, None);
        g.play(GameTransition::Start).unwrap();
        g.override_hands([
            hand("2C 3C 4C 5C 2D 3D 4D 5D 2H 3H 4H 2S 3S"),
            hand("6C 7C 8C 9C 6D 7D 8D 9D 5H 6H 7H 4S 5S"),
            hand("TC JC QC KC TD JD QD KD 8H 9H TH 6S 7S"),
            hand("AC AD JH QH KH AH AS 8S 9S TS JS QS KS"),
        ]);
        for _ in 0..4 {
            g.play(GameTransition::Bet(3)).unwrap();
        }
        g
    }

    #[test]
    fn replay_rejects_off_suit_when_player_can_follow() {
        use crate::cards::{Rank, Suit};
        let g = rig_game_with_fixed_hands();
        let encoded = encode(&g);
        let mut parsed = decode(&encoded).unwrap();
        let two_c = Card {
            rank: Rank::Two,
            suit: Suit::Club,
        };
        let six_d = Card {
            rank: Rank::Six,
            suit: Suit::Diamond,
        };
        parsed.rounds[0].tricks.push(vec![two_c, six_d]);
        let err = replay(&parsed).expect_err("replay should reject off-suit follow");
        assert!(matches!(
            err,
            ReplayError::Transition {
                err: crate::TransitionError::CardIncorrectSuit,
                ..
            }
        ));
    }

    #[test]
    fn replay_rejects_spade_lead_before_spades_broken() {
        use crate::cards::{Rank, Suit};
        let g = rig_game_with_fixed_hands();
        let encoded = encode(&g);
        let mut parsed = decode(&encoded).unwrap();
        let two_s = Card {
            rank: Rank::Two,
            suit: Suit::Spade,
        };
        parsed.rounds[0].tricks.push(vec![two_s]);
        let err = replay(&parsed).expect_err("replay should reject spade lead");
        assert!(matches!(
            err,
            ReplayError::Transition {
                err: crate::TransitionError::SpadesNotBroken,
                ..
            }
        ));
    }

    #[test]
    fn replay_rejects_termination_inprogress_when_actually_complete() {
        // Mirror of replay_rejects_termination_mismatch: the transcript claims
        // InProgress but the recorded plays terminate the game, so finalize
        // sees actual=Completed and declared=InProgress.
        let g = build_short_game();
        let encoded = encode(&g)
            .replace(
                "[Termination \"Completed\"]",
                "[Termination \"InProgress\"]",
            )
            .replace(
                &format!(
                    "[Result \"{} {}\"]",
                    g.get_team_a_score().unwrap(),
                    g.get_team_b_score().unwrap()
                ),
                "[Result \"*\"]",
            );
        let parsed = decode(&encoded).unwrap();
        assert_eq!(parsed.termination, Termination::InProgress);
        assert!(matches!(
            replay(&parsed),
            Err(ReplayError::TerminationMismatch {
                declared: Termination::InProgress,
                actual: Termination::Completed,
            })
        ));
    }
}
