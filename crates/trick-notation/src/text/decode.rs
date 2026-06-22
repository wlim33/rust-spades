use crate::model::Model;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("placeholder")]
    Placeholder,
}

/// Parse canonical text into a model. Implemented in Task 5.
pub fn from_text(_text: &str) -> Result<Model, ParseError> {
    Err(ParseError::Placeholder)
}
