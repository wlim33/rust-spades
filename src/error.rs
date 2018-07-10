use std::error::Error;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum GetError {
    InvalidUuid
}

#[derive(Debug, PartialEq)]
pub enum TransitionError {
    AlreadyStarted,
    NotStarted,
    CardInBettingStage,
    BetInTrickStage,
    CompletedGame,
    CardNotInHand
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
                write!(f, "Error: Attempted to place a bet while game is in not stage.")},
            TransitionError::CompletedGame => {
                write!(f, "Error: Attempted to play a completed game.")},
            TransitionError::CardNotInHand => {
                write!(f, "Error: Attempted to play a card not in hand")},
        }
    }
}

impl Error for TransitionError {
    fn description(&self) -> &str {
        "A transition error occured."
    }
    fn cause(&self) -> Option<&Error> {
        Some(self)
    }
}

#[derive(Debug, PartialEq)]
pub enum Success {
    Bet,
    BetComplete,
    Trick,
    PlayCard,
    GameOver,
    Start
}

