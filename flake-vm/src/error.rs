//! VM and compile errors.

use flake_ast::{Source, Span, render};
use thiserror::Error;

#[derive(Debug, Clone, Error)]
#[error("{message}")]
pub struct VmError {
    pub span: Span,
    pub message: String,
}

impl VmError {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }

    pub fn display(&self, source: &Source) -> String {
        render(source, self.span, "error", &self.message)
    }
}

#[derive(Debug, Error)]
pub enum ExecuteError {
    #[error(transparent)]
    Parse(#[from] flake_parser::ParseError),
    #[error(transparent)]
    Compile(#[from] VmError),
    #[error(transparent)]
    Runtime(VmError),
}

impl ExecuteError {
    pub fn display(&self, source: &Source) -> String {
        match self {
            Self::Parse(err) => render(source, err.span, "error", &err.message),
            Self::Compile(err) | Self::Runtime(err) => err.display(source),
        }
    }

    pub fn span(&self) -> Option<Span> {
        match self {
            Self::Parse(err) => Some(err.span),
            Self::Compile(err) | Self::Runtime(err) => Some(err.span),
        }
    }
}
