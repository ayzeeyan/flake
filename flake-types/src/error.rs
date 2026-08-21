//! Type errors.

use flake_ast::{Source, Span, render};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct TypeError {
    pub span: Span,
    pub message: String,
}

impl TypeError {
    #[must_use]
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn display(&self, source: &Source) -> String {
        render(source, self.span, "error", &self.message)
    }
}

/// Parse or type error from `check`.
#[derive(Debug, Error)]
pub enum CheckError {
    #[error(transparent)]
    Parse(#[from] flake_parser::ParseError),
    #[error(transparent)]
    Type(#[from] TypeError),
    /// Type error whose span belongs to an imported file.
    #[error("{error}")]
    TypeIn {
        origin: Source,
        error: TypeError,
    },
    #[error(transparent)]
    Resolve(#[from] flake_parser::ResolveError),
}

impl CheckError {
    #[must_use]
    pub fn display(&self, source: &Source) -> String {
        match self {
            Self::Parse(err) => render(source, err.span, "error", &err.message),
            Self::Type(err) => err.display(source),
            Self::TypeIn { origin, error } => error.display(origin),
            Self::Resolve(err) => {
                let src = err.origin.as_ref().unwrap_or(source);
                render(src, err.span, "error", &err.message)
            }
        }
    }
}
