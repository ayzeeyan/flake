//! Parse errors.

use flake_ast::Span;
use flake_lexer::LexError;
use thiserror::Error;

/// A failure to parse Flake source.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct ParseError {
    pub span: Span,
    pub message: String,
}

impl ParseError {
    #[must_use]
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}

impl From<LexError> for ParseError {
    fn from(err: LexError) -> Self {
        Self {
            span: err.span,
            message: err.message,
        }
    }
}
