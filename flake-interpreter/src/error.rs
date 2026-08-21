//! Runtime and driver errors for the interpreter.

use std::io;

use flake_ast::{Source, Span, render};
use flake_parser::{ParseError, ResolveError};
use thiserror::Error;

/// An error that occurred while evaluating Flake code.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct RuntimeError {
    pub span: Span,
    pub message: String,
}

impl RuntimeError {
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

/// Failure to parse or run a program.
#[derive(Debug, Error)]
pub enum RunError {
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
}

impl RunError {
    #[must_use]
    pub fn display(&self, source: &Source) -> String {
        match self {
            Self::Parse(err) => render(source, err.span, "error", &err.message),
            Self::Resolve(err) => {
                let src = err.origin.as_ref().unwrap_or(source);
                render(src, err.span, "error", &err.message)
            }
            Self::Runtime(err) => err.display(source),
            Self::Io(err) => format!("error: {err}\n"),
        }
    }

    #[must_use]
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::Parse(err) => Some(err.span),
            Self::Resolve(err) => Some(err.span),
            Self::Runtime(err) => Some(err.span),
            Self::Io(_) => None,
        }
    }
}
