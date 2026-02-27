use crate::{Card, Game};
use rand::seq::SliceRandom;
use rand::Rng;

/// Trait for AI decision-making in Spades. Implement this to create new AI strategies.
pub trait AiStrategy: Send + Sync {
    /// Choose a bet for the given player index. Called during Betting state.
    fn choose_bet(&self, game: &Game, player_index: usize) -> i32;
    /// Choose a card to play for the given player index. Called during Trick state.
    fn choose_card(&self, game: &Game, player_index: usize) -> Card;
}

/// AI strategy that picks random valid moves.
pub struct RandomStrategy;

impl AiStrategy for RandomStrategy {
    fn choose_bet(&self, _game: &Game, _player_index: usize) -> i32 {
        let mut rng = rand::thread_rng();
        rng.gen_range(1..=4)
    }

    fn choose_card(&self, game: &Game, _player_index: usize) -> Card {
        let mut rng = rand::thread_rng();
        let legal_cards = game.get_legal_cards().expect("AI should only be called in Trick state");
        legal_cards.choose(&mut rng).expect("hand should not be empty").clone()
    }
}
