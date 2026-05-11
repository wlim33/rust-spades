use crate::cards::{get_trick_winner, Card};
use crate::{Game, GameTransition, State};

use super::{ReplayError, Round, Termination, Transcript};

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

    game.play(GameTransition::Start).map_err(|e| ReplayError::Transition {
        round: 0,
        trick: None,
        seat: 0,
        err: e,
    })?;

    // Seed the engine with the transcript's declared hands rather than the
    // randomly-dealt ones from Start.
    game.override_hands(t.rounds[0].hands.clone());

    for (r_idx, round) in t.rounds.iter().enumerate() {
        for (i, &b) in round.bets.iter().enumerate() {
            game.play(GameTransition::Bet(b)).map_err(|e| ReplayError::Transition {
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
            verify_dealt_hands(&game, round, r_idx)?;
            break;
        }

        verify_dealt_hands(&game, round, r_idx)?;

        let mut lead = 0usize;
        for (t_idx, trick) in round.tricks.iter().enumerate() {
            for (i, &card) in trick.iter().enumerate() {
                let seat = (lead + i) % 4;
                game.play(GameTransition::Card(card)).map_err(|e| {
                    ReplayError::Transition {
                        round: r_idx,
                        trick: Some(t_idx),
                        seat,
                        err: e,
                    }
                })?;
            }
            if trick.len() == 4 {
                let mut by_seat = [Card { rank: crate::cards::Rank::Two, suit: crate::cards::Suit::Club }; 4];
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

fn verify_dealt_hands(g: &Game, round: &Round, r_idx: usize) -> Result<(), ReplayError> {
    let names = g.get_player_names();
    for seat in 0..4 {
        let pid = names[seat].0;
        let actual = g
            .get_hand_by_player_id(pid)
            .map_err(|_| ReplayError::HandMismatch { round: r_idx, seat })?;
        let mut a: Vec<Card> = actual.clone();
        a.sort();
        let mut d = round.hands[seat].clone();
        d.sort();
        if a != d {
            return Err(ReplayError::HandMismatch { round: r_idx, seat });
        }
    }
    Ok(())
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
        let a = game.get_team_a_score().copied().unwrap_or(0);
        let b = game.get_team_b_score().copied().unwrap_or(0);
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
            replayed.get_team_a_score().copied().unwrap_or(0),
            g.get_team_a_score().copied().unwrap_or(0)
        );
        assert_eq!(
            replayed.get_team_b_score().copied().unwrap_or(0),
            g.get_team_b_score().copied().unwrap_or(0)
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
        // Force the first card of round 0 trick 0 to AS regardless of what the
        // first player actually held — should fail either HandMismatch or
        // Transition depending on whether AS was already in the hand.
        parsed.rounds[0].tricks[0][0] = Card {
            rank: crate::cards::Rank::Ace,
            suit: crate::cards::Suit::Spade,
        };
        match replay(&parsed) {
            Err(ReplayError::Transition { .. }) | Err(ReplayError::HandMismatch { .. }) => {}
            other => panic!("expected replay error, got {:?}", other),
        }
    }
}
