use crate::cards::{Card, Suit, Rank};
use crate::result::{TransitionError, GetError};
use crate::{Game, GameTransition, TimerConfig};
use crate::game_state::State;
use uuid::Uuid;
use ntest::test_case;

/// Helper: create a started game with known hands for controlled testing.
fn make_started_game() -> (Game, [Uuid; 4]) {
    let player_ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    let mut g = Game::new(Uuid::new_v4(), player_ids, 500, None);
    g.play(GameTransition::Start).unwrap();
    (g, player_ids)
}

/// Helper: create a game and play through all bets so it's in Trick(0) state.
fn make_game_in_trick_state() -> (Game, [Uuid; 4]) {
    let (mut g, pids) = make_started_game();
    for _ in 0..4 {
        g.play(GameTransition::Bet(3)).unwrap();
    }
    assert!(matches!(*g.get_state(), State::Trick(0)));
    (g, pids)
}

// ── get_current_player_id edge cases ──

#[test]
fn test_get_current_player_id_not_started() {
    let g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    assert_eq!(g.get_current_player_id(), Err(GetError::GameNotStarted));
}

#[test]
fn test_get_current_player_id_completed() {
    let mut g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    g.set_state(State::Completed);
    assert_eq!(g.get_current_player_id(), Err(GetError::GameCompleted));
}

// ── get_current_hand edge cases ──

#[test]
fn test_get_current_hand_not_started() {
    let g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    assert_eq!(g.get_current_hand(), Err(GetError::GameNotStarted));
}

#[test]
fn test_get_current_hand_completed() {
    let mut g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    g.set_state(State::Completed);
    assert_eq!(g.get_current_hand(), Err(GetError::GameCompleted));
}

// ── get_leading_suit edge cases ──

#[test]
fn test_get_leading_suit_not_started() {
    let g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    assert_eq!(g.get_leading_suit(), Err(GetError::GameNotStarted));
}

#[test]
fn test_get_leading_suit_completed() {
    let mut g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    g.set_state(State::Completed);
    assert_eq!(g.get_leading_suit(), Err(GetError::GameCompleted));
}

#[test]
fn test_get_leading_suit_betting() {
    let (g, _) = make_started_game();
    assert_eq!(g.get_leading_suit(), Err(GetError::Unknown));
}

#[test]
fn test_get_leading_suit_trick() {
    let (g, _) = make_game_in_trick_state();
    assert!(g.get_leading_suit().is_ok());
}

// ── get_current_trick_cards edge cases ──

#[test_case("NotStarted")]
#[test_case("Completed")]
#[test_case("Aborted")]
#[test_case("Betting")]
fn get_current_trick_cards_error_states(state_name: &str) {
    let mut g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    match state_name {
        "NotStarted" => {},
        "Completed" => g.set_state(State::Completed),
        "Aborted" => g.set_state(State::Aborted),
        "Betting" => { g.play(GameTransition::Start).unwrap(); },
        _ => unreachable!(),
    }
    assert!(g.get_current_trick_cards().is_err());
}

#[test]
fn test_get_current_trick_cards_in_trick() {
    let (g, _) = make_game_in_trick_state();
    let cards = g.get_current_trick_cards().unwrap();
    assert_eq!(cards.len(), 4); // 4-element array
}

// ── get_hand (deprecated) ──

#[test_case(0)]
#[test_case(1)]
#[test_case(2)]
#[test_case(3)]
#[test_case(99)]
fn deprecated_get_hand(player_idx: usize) {
    let (g, _) = make_started_game();
    #[allow(deprecated)]
    let result = g.get_hand(player_idx);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 13);
}

// ── get_winner_ids ──

#[test]
fn test_get_winner_ids_not_completed() {
    let g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    assert_eq!(g.get_winner_ids(), Err(GetError::GameNotCompleted));
}

#[test]
fn test_get_winner_ids_team_a_wins() {
    let pids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    let mut g = Game::new(Uuid::new_v4(), pids, 500, None);
    g.set_state(State::Completed);
    g.scoring.team_a.cumulative_points = 500;
    g.scoring.team_b.cumulative_points = 100;
    let (w1, w2) = g.get_winner_ids().unwrap();
    assert_eq!(*w1, pids[0]);
    assert_eq!(*w2, pids[2]);
}

#[test]
fn test_get_winner_ids_team_b_wins() {
    let pids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    let mut g = Game::new(Uuid::new_v4(), pids, 500, None);
    g.set_state(State::Completed);
    g.scoring.team_a.cumulative_points = 100;
    g.scoring.team_b.cumulative_points = 500;
    let (w1, w2) = g.get_winner_ids().unwrap();
    assert_eq!(*w1, pids[1]);
    assert_eq!(*w2, pids[3]);
}

#[test]
fn test_get_winner_ids_tie_returns_error() {
    let mut g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    g.set_state(State::Completed);
    g.scoring.team_a.cumulative_points = 500;
    g.scoring.team_b.cumulative_points = 500;
    assert_eq!(g.get_winner_ids(), Err(GetError::GameNotCompleted));
}

// ── get_legal_cards ──

#[test]
fn test_get_legal_cards_not_in_trick() {
    let (g, _) = make_started_game();
    assert_eq!(g.get_legal_cards(), Err(GetError::Unknown));
}

#[test]
fn test_get_legal_cards_first_card_all_legal() {
    let (g, _) = make_game_in_trick_state();
    let legal = g.get_legal_cards().unwrap();
    let hand = g.get_current_hand().unwrap();
    assert_eq!(legal.len(), hand.len());
}

#[test]
fn test_get_legal_cards_must_follow_suit() {
    let (mut g, _) = make_game_in_trick_state();
    // Play a card from player A to set leading suit
    let hand = g.get_current_hand().unwrap().clone();
    let first_card = hand[0].clone();
    let leading_suit = first_card.suit;
    g.play(GameTransition::Card(first_card)).unwrap();

    // Now player B must follow suit if possible
    let legal = g.get_legal_cards().unwrap();
    let hand_b = g.get_current_hand().unwrap();
    let has_leading = hand_b.iter().any(|c| c.suit == leading_suit);
    if has_leading {
        assert!(legal.iter().all(|c| c.suit == leading_suit));
    } else {
        assert_eq!(legal.len(), hand_b.len());
    }
}

// ── Timer-related getters/setters ──

#[test]
fn test_timer_config_and_clocks() {
    let tc = TimerConfig { initial_time_secs: 300, increment_secs: 5 };
    let g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, Some(tc));
    assert_eq!(g.get_timer_config(), Some(&tc));
    assert!(g.get_player_clocks().is_some());
    assert_eq!(g.get_player_clocks().unwrap().remaining_ms, [300_000; 4]);
}

#[test]
fn test_timer_config_none() {
    let g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    assert!(g.get_timer_config().is_none());
    assert!(g.get_player_clocks().is_none());
}

#[test]
fn test_player_clocks_mut() {
    let tc = TimerConfig { initial_time_secs: 300, increment_secs: 5 };
    let mut g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, Some(tc));
    if let Some(clocks) = g.get_player_clocks_mut() {
        clocks.remaining_ms[0] = 100_000;
    }
    assert_eq!(g.get_player_clocks().unwrap().remaining_ms[0], 100_000);
}

#[test]
fn test_get_current_player_index_num() {
    let (g, _) = make_started_game();
    assert_eq!(g.get_current_player_index_num(), 0);
}

#[test]
fn test_is_first_round_betting() {
    let (g, _) = make_started_game();
    assert!(g.is_first_round_betting());
}

#[test]
fn test_is_first_round_betting_false_in_trick() {
    let (g, _) = make_game_in_trick_state();
    assert!(!g.is_first_round_betting());
}

#[test]
fn test_turn_started_at_epoch_ms() {
    let mut g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    assert!(g.get_turn_started_at_epoch_ms().is_none());
    g.set_turn_started_at_epoch_ms(Some(12345));
    assert_eq!(g.get_turn_started_at_epoch_ms(), Some(12345));
    g.set_turn_started_at_epoch_ms(None);
    assert!(g.get_turn_started_at_epoch_ms().is_none());
}

#[test]
fn test_set_state() {
    let mut g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    assert_eq!(*g.get_state(), State::NotStarted);
    g.set_state(State::Aborted);
    assert_eq!(*g.get_state(), State::Aborted);
}

// ── Play errors: CompletedGame, CardNotInHand, CardIncorrectSuit, BetInTrickStage ──

#[test]
fn test_bet_on_completed_game() {
    let mut g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    g.set_state(State::Completed);
    assert_eq!(g.play(GameTransition::Bet(3)), Err(TransitionError::CompletedGame));
}

#[test]
fn test_card_on_completed_game() {
    let mut g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    g.set_state(State::Completed);
    let card = Card { suit: Suit::Heart, rank: Rank::Ace };
    assert_eq!(g.play(GameTransition::Card(card)), Err(TransitionError::CompletedGame));
}

#[test]
fn test_bet_in_trick_stage() {
    let (mut g, _) = make_game_in_trick_state();
    assert_eq!(g.play(GameTransition::Bet(3)), Err(TransitionError::BetInTrickStage));
}

#[test]
fn test_card_not_in_hand() {
    let (mut g, _) = make_game_in_trick_state();
    // Try to play a card that's definitely not in the current hand
    let fake_card = Card { suit: Suit::Blank, rank: Rank::Blank };
    assert_eq!(g.play(GameTransition::Card(fake_card)), Err(TransitionError::CardNotInHand));
}

#[test]
fn test_card_incorrect_suit() {
    let (mut g, _) = make_game_in_trick_state();
    // Play first card to establish leading suit
    let hand = g.get_current_hand().unwrap().clone();
    let first_card = hand[0].clone();
    let leading_suit = first_card.suit;
    g.play(GameTransition::Card(first_card)).unwrap();

    // Try to play a card of wrong suit from player B's hand (if they have the leading suit)
    let hand_b = g.get_current_hand().unwrap().clone();
    let has_leading = hand_b.iter().any(|c| c.suit == leading_suit);
    if has_leading {
        // Find a card that is NOT the leading suit
        if let Some(wrong_card) = hand_b.iter().find(|c| c.suit != leading_suit) {
            assert_eq!(
                g.play(GameTransition::Card(wrong_card.clone())),
                Err(TransitionError::CardIncorrectSuit)
            );
        }
    }
}

// ── get_hand_by_player_id with invalid UUID ──

#[test]
fn test_get_hand_by_player_id_invalid() {
    let (g, _) = make_started_game();
    assert_eq!(g.get_hand_by_player_id(Uuid::new_v4()), Err(GetError::InvalidUuid));
}

#[test]
fn test_get_hand_by_player_id_all_players() {
    let (g, pids) = make_started_game();
    for pid in &pids {
        let hand = g.get_hand_by_player_id(*pid).unwrap();
        assert_eq!(hand.len(), 13);
    }
}

// ── get_player_names ──

#[test]
fn test_get_player_names_default_none() {
    let pids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    let g = Game::new(Uuid::new_v4(), pids, 500, None);
    let names = g.get_player_names();
    for (id, name) in &names {
        assert!(pids.contains(id));
        assert!(name.is_none());
    }
}

#[test]
fn test_set_player_name_invalid_id() {
    let mut g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    assert_eq!(
        g.set_player_name(Uuid::new_v4(), Some("Test".to_string())),
        Err(GetError::InvalidUuid)
    );
}

// ── Score getters in not-started state ──

#[test]
fn test_score_getters_not_started() {
    let g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    assert_eq!(g.get_team_a_score(), Err(GetError::GameNotStarted));
    assert_eq!(g.get_team_b_score(), Err(GetError::GameNotStarted));
    assert_eq!(g.get_team_a_bags(), Err(GetError::GameNotStarted));
    assert_eq!(g.get_team_b_bags(), Err(GetError::GameNotStarted));
}
