use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransitionSuccess {
    Bet,
    BetComplete,
    Trick,
    PlayCard,
    GameOver,
    Start,
    /// The game was abandoned via a `GameTransition::Abort`.
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
pub enum GetError {
    #[error("Error: Attempted to retrieve by an invalid Uuid")]
    InvalidUuid,
    #[error("Error: Game not started yet.")]
    GameNotStarted,
    #[error("Error: Game is completed.")]
    GameCompleted,
    #[error("Error: Game is still ongoing.")]
    GameNotCompleted,
    #[error("Error: Unknown get error occurred.")]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
pub enum TransitionError {
    #[error("Error: Attempted to start a game already started.")]
    AlreadyStarted,
    #[error("Error: Attempted to play a game not started yet.")]
    NotStarted,
    #[error("Error: Attempted to play a card while game is in betting stage.")]
    CardInBettingStage,
    #[error("Error: Attempted to place a bet while game is in trick stage.")]
    BetInTrickStage,
    #[error("Error: Attempted to play a completed game.")]
    CompletedGame,
    #[error("Error: Attempted to play a card not in hand.")]
    CardNotInHand,
    #[error("Error: Attempted to play a card of the wrong suit.")]
    CardIncorrectSuit,
    #[error("Error: Cannot lead a spade until spades have been broken.")]
    SpadesNotBroken,
    #[error("Error: Bet must be between 0 and 13 inclusive.")]
    InvalidBet,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntest::test_case;
    use std::error::Error;

    #[test_case("InvalidUuid")]
    #[test_case("GameNotStarted")]
    #[test_case("GameCompleted")]
    #[test_case("GameNotCompleted")]
    #[test_case("Unknown")]
    fn get_error_display_contains_error(variant_name: &str) {
        let err = match variant_name {
            "InvalidUuid" => GetError::InvalidUuid,
            "GameNotStarted" => GetError::GameNotStarted,
            "GameCompleted" => GetError::GameCompleted,
            "GameNotCompleted" => GetError::GameNotCompleted,
            "Unknown" => GetError::Unknown,
            _ => unreachable!(),
        };
        let msg = format!("{}", err);
        assert!(
            msg.starts_with("Error:"),
            "GetError::{} display should start with 'Error:', got: {}",
            variant_name,
            msg
        );
    }

    #[test_case("AlreadyStarted")]
    #[test_case("NotStarted")]
    #[test_case("CardInBettingStage")]
    #[test_case("BetInTrickStage")]
    #[test_case("CompletedGame")]
    #[test_case("CardNotInHand")]
    #[test_case("CardIncorrectSuit")]
    #[test_case("SpadesNotBroken")]
    #[test_case("InvalidBet")]
    fn transition_error_display_contains_error(variant_name: &str) {
        let err = match variant_name {
            "AlreadyStarted" => TransitionError::AlreadyStarted,
            "NotStarted" => TransitionError::NotStarted,
            "CardInBettingStage" => TransitionError::CardInBettingStage,
            "BetInTrickStage" => TransitionError::BetInTrickStage,
            "CompletedGame" => TransitionError::CompletedGame,
            "CardNotInHand" => TransitionError::CardNotInHand,
            "CardIncorrectSuit" => TransitionError::CardIncorrectSuit,
            "SpadesNotBroken" => TransitionError::SpadesNotBroken,
            "InvalidBet" => TransitionError::InvalidBet,
            _ => unreachable!(),
        };
        let msg = format!("{}", err);
        assert!(
            msg.starts_with("Error:"),
            "TransitionError::{} display should start with 'Error:', got: {}",
            variant_name,
            msg
        );
    }

    #[test]
    fn transition_error_implements_std_error() {
        let err = TransitionError::NotStarted;
        assert_eq!(
            err.to_string(),
            "Error: Attempted to play a game not started yet."
        );
        assert!(err.source().is_none());
    }
}
