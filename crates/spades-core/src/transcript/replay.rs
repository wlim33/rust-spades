use crate::Game;

use super::{ReplayError, Transcript};

pub fn replay(_t: &Transcript) -> Result<Game, ReplayError> {
    Err(ReplayError::TimerHalfSpecified)
}
