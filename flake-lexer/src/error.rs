//! Lexical error type.

use flake_ast::Span;
use thiserror::Error;

/// A failure to tokenize Flake source.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct LexError {
    pub span: Span,
    pub message: String,
}

impl LexError {
    #[must_use]
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}
