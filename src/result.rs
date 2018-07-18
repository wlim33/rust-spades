use std::error::Error;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum TransitionSuccess {
    Bet,
    BetComplete,
    Trick,
    PlayCard,
    GameOver,
    Start
}

#[derive(Debug, PartialEq)]
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

#[derive(Debug, PartialEq)]
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
                write!(f, "Error: Attempted to place a bet while game is in not stage.")},
            TransitionError::CompletedGame => {
                write!(f, "Error: Attempted to play a completed game.")},
            TransitionError::CardNotInHand => {
                write!(f, "Error: Attempted to play a card not in hand.")},
            TransitionError::CardIncorrectSuit => {
                write!(f, "Error: Attempted to play a of the wrong suit.")},
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