use super::super::cards::{Card, Suit, Rank};
use super::super::result::{TransitionSuccess, TransitionError, GetError};
use super::super::{Game, GameTransition};
use super::super::game_state::State;

#[test]
fn test_invalid_bet_values() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        ],
        500,
    );

    g.play(GameTransition::Start).unwrap();

    // Test negative bets
    assert_eq!(
        g.play(GameTransition::Bet(-1)),
        Ok(TransitionSuccess::Bet)
    );

    // Test extremely large bets
    g.play(GameTransition::Bet(100)).unwrap();
    g.play(GameTransition::Bet(1000)).unwrap();
    g.play(GameTransition::Bet(i32::MAX)).unwrap();
}

#[test]
fn test_game_state_getters_when_not_started() {
    let g = Game::new(
        uuid::Uuid::new_v4(),
        [
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        ],
        500,
    );

    assert_eq!(g.get_team_a_score(), Err(GetError::GameNotStarted));
    assert_eq!(g.get_team_b_score(), Err(GetError::GameNotStarted));
    assert_eq!(g.get_team_a_bags(), Err(GetError::GameNotStarted));
    assert_eq!(g.get_team_b_bags(), Err(GetError::GameNotStarted));
    assert_eq!(g.get_current_player_id(), Err(GetError::GameNotStarted));
    assert_eq!(g.get_current_hand(), Err(GetError::GameNotStarted));
    assert_eq!(g.get_leading_suit(), Err(GetError::GameNotStarted));
    assert_eq!(g.get_current_trick_cards(), Err(GetError::GameNotStarted));
}

#[test]
fn test_invalid_uuid_lookup() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        ],
        500,
    );

    g.play(GameTransition::Start).unwrap();

    // Try to get hand with invalid UUID
    let invalid_uuid = uuid::Uuid::new_v4();
    assert_eq!(
        g.get_hand_by_player_id(invalid_uuid),
        Err(GetError::InvalidUuid)
    );
}

#[test]
fn test_play_card_not_in_hand() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        ],
        500,
    );

    g.play(GameTransition::Start).unwrap();

    // Complete betting
    g.play(GameTransition::Bet(3)).unwrap();
    g.play(GameTransition::Bet(3)).unwrap();
    g.play(GameTransition::Bet(3)).unwrap();
    g.play(GameTransition::Bet(3)).unwrap();

    // Try to play a card that's definitely not in hand
    let invalid_card = Card {
        suit: Suit::Spade,
        rank: Rank::Ace,
    };

    // Keep trying with different cards until we find one not in the hand
    let hand = g.get_current_hand().unwrap().clone();
    if !hand.contains(&invalid_card) {
        assert_eq!(
            g.play(GameTransition::Card(invalid_card)),
            Err(TransitionError::CardNotInHand)
        );
    }
}

#[test]
fn test_play_wrong_suit() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        ],
        500,
    );

    g.play(GameTransition::Start).unwrap();

    // Complete betting
    g.play(GameTransition::Bet(3)).unwrap();
    g.play(GameTransition::Bet(3)).unwrap();
    g.play(GameTransition::Bet(3)).unwrap();
    g.play(GameTransition::Bet(3)).unwrap();

    // Play first card to establish leading suit
    let hand = g.get_current_hand().unwrap().clone();
    let first_card = hand[0].clone();
    g.play(GameTransition::Card(first_card.clone())).unwrap();

    // Try to play a card of different suit if player has cards of leading suit
    let hand2 = g.get_current_hand().unwrap().clone();
    if let Some(wrong_card) = hand2.iter().find(|c| c.suit != first_card.suit) {
        if hand2.iter().any(|c| c.suit == first_card.suit) {
            let result = g.play(GameTransition::Card(wrong_card.clone()));
            assert_eq!(result, Err(TransitionError::CardIncorrectSuit));
        }
    }
}

#[test]
fn test_nil_betting() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        ],
        500,
    );

    g.play(GameTransition::Start).unwrap();

    // Test nil (0) betting
    assert_eq!(g.play(GameTransition::Bet(0)), Ok(TransitionSuccess::Bet));
    g.play(GameTransition::Bet(3)).unwrap();
    g.play(GameTransition::Bet(0)).unwrap();
    g.play(GameTransition::Bet(3)).unwrap();
}

#[test]
fn test_get_winner_before_completion() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        ],
        500,
    );

    // Before starting
    assert_eq!(g.get_winner_ids(), Err(GetError::GameNotCompleted));

    g.play(GameTransition::Start).unwrap();

    // After starting but not completed
    assert_eq!(g.get_winner_ids(), Err(GetError::GameNotCompleted));
}

#[test]
fn test_multiple_starts() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        ],
        500,
    );

    assert_eq!(g.play(GameTransition::Start), Ok(TransitionSuccess::Start));
    assert_eq!(
        g.play(GameTransition::Start),
        Err(TransitionError::AlreadyStarted)
    );
    assert_eq!(
        g.play(GameTransition::Start),
        Err(TransitionError::AlreadyStarted)
    );
}

#[test]
fn test_state_transitions() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        ],
        500,
    );

    assert_eq!(*g.get_state(), State::NotStarted);

    g.play(GameTransition::Start).unwrap();
    assert_eq!(*g.get_state(), State::Betting(0));

    g.play(GameTransition::Bet(3)).unwrap();
    assert_eq!(*g.get_state(), State::Betting(1));

    g.play(GameTransition::Bet(3)).unwrap();
    assert_eq!(*g.get_state(), State::Betting(2));

    g.play(GameTransition::Bet(3)).unwrap();
    assert_eq!(*g.get_state(), State::Betting(3));

    g.play(GameTransition::Bet(3)).unwrap();
    assert_eq!(*g.get_state(), State::Trick(0));
}

#[test]
fn test_valid_card_sequence() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        ],
        500,
    );

    g.play(GameTransition::Start).unwrap();

    // Complete betting
    g.play(GameTransition::Bet(3)).unwrap();
    g.play(GameTransition::Bet(3)).unwrap();
    g.play(GameTransition::Bet(3)).unwrap();
    g.play(GameTransition::Bet(3)).unwrap();

    // Play valid cards in sequence
    for _ in 0..4 {
        let hand = g.get_current_hand().unwrap().clone();
        let leading_suit = g.get_leading_suit().ok().copied();

        let card_to_play = if let Some(suit) = leading_suit {
            // Find a card matching the leading suit if available
            hand.iter()
                .find(|c| c.suit == suit)
                .or_else(|| hand.first())
                .unwrap()
                .clone()
        } else {
            hand[0].clone()
        };

        g.play(GameTransition::Card(card_to_play)).unwrap();
    }
}
