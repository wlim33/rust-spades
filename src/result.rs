use std::error::Error;
use std::fmt;
use serde::{Serialize, Deserialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum TransitionSuccess {
    Bet,
    BetComplete,
    Trick,
    PlayCard,
    GameOver,
    Start
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum GetError {
    InvalidUuid,
    GameNotStarted,
    GameCompleted,
    GameNotCompleted,
    Unknown
}

impl fmt::Display for GetError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            GetError::InvalidUuid => {
                write!(f, "Error: Attempted to retrieve by an invalid Uuid")},
            GetError::GameNotStarted => {
                write!(f, "Error: Game not started yet.")},
            GetError::GameCompleted => {
                write!(f, "Error: Game is completed.")},
            GetError::GameNotCompleted => {
                write!(f, "Error: Game is still ongoing.")},
            GetError::Unknown => {
                write!(f, "Error: Unknown get error occurred.")},
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum TransitionError {
    AlreadyStarted,
    NotStarted,
    CardInBettingStage,
    BetInTrickStage,
    CompletedGame,
    CardNotInHand,
    CardIncorrectSuit
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            TransitionError::AlreadyStarted => {
                write!(f, "Error: Attempted to start a game already started.")},
            TransitionError::NotStarted => {
                write!(f, "Error: Attempted to play a game not started yet.")},
            TransitionError::CardInBettingStage => {
                write!(f, "Error: Attempted to play a card while game is in betting stage.")},
            TransitionError::BetInTrickStage => {
                write!(f, "Error: Attempted to place a bet while game is in trick stage.")},
            TransitionError::CompletedGame => {
                write!(f, "Error: Attempted to play a completed game.")},
            TransitionError::CardNotInHand => {
                write!(f, "Error: Attempted to play a card not in hand.")},
            TransitionError::CardIncorrectSuit => {
                write!(f, "Error: Attempted to play a card of the wrong suit.")},
        }
    }
}

impl Error for TransitionError {}

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
        assert!(msg.starts_with("Error:"), "GetError::{} display should start with 'Error:', got: {}", variant_name, msg);
    }

    #[test_case("AlreadyStarted")]
    #[test_case("NotStarted")]
    #[test_case("CardInBettingStage")]
    #[test_case("BetInTrickStage")]
    #[test_case("CompletedGame")]
    #[test_case("CardNotInHand")]
    #[test_case("CardIncorrectSuit")]
    fn transition_error_display_contains_error(variant_name: &str) {
        let err = match variant_name {
            "AlreadyStarted" => TransitionError::AlreadyStarted,
            "NotStarted" => TransitionError::NotStarted,
            "CardInBettingStage" => TransitionError::CardInBettingStage,
            "BetInTrickStage" => TransitionError::BetInTrickStage,
            "CompletedGame" => TransitionError::CompletedGame,
            "CardNotInHand" => TransitionError::CardNotInHand,
            "CardIncorrectSuit" => TransitionError::CardIncorrectSuit,
            _ => unreachable!(),
        };
        let msg = format!("{}", err);
        assert!(msg.starts_with("Error:"), "TransitionError::{} display should start with 'Error:', got: {}", variant_name, msg);
    }

    #[test]
    fn transition_error_implements_std_error() {
        let err = TransitionError::NotStarted;
        assert_eq!(err.to_string(), "Error: Attempted to play a game not started yet.");
        assert!(err.source().is_none());
    }
}