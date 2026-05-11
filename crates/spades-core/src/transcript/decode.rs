use super::{DecodeError, Transcript};

pub fn decode(_text: &str) -> Result<Transcript, DecodeError> {
    Err(DecodeError::UnexpectedEof)
}
