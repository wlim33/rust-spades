use crate::ai::{AiStrategy, RandomStrategy};
use crate::{Game, GameTransition, State};
use uuid::Uuid;

#[test]
fn test_random_strategy_bet_in_range() {
    let strategy = RandomStrategy;
    let game = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    for _ in 0..100 {
        let bet = strategy.choose_bet(&game, 0);
        assert!(bet >= 1 && bet <= 4, "bet {} out of range", bet);
    }
}

#[test]
fn test_random_strategy_card_is_legal() {
    let strategy = RandomStrategy;
    let mut game = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    game.play(GameTransition::Start).unwrap();
    for _ in 0..4 {
        game.play(GameTransition::Bet(2)).unwrap();
    }
    assert!(matches!(game.get_state(), State::Trick(_)));
    let legal_cards = game.get_legal_cards().unwrap();
    let chosen = strategy.choose_card(&game, 0);
    assert!(legal_cards.contains(&chosen), "chosen card not in legal cards");
}

#[test]
fn test_ai_strategy_is_object_safe() {
    let strategy: Box<dyn AiStrategy> = Box::new(RandomStrategy);
    let game = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 500, None);
    let _bet = strategy.choose_bet(&game, 0);
}
