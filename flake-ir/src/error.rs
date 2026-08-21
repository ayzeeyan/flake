//! IR lowering errors.

use flake_ast::{Source, Span, render};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IrError {
    #[error(transparent)]
    Parse(#[from] flake_parser::ParseError),
    #[error(transparent)]
    Resolve(#[from] flake_parser::ResolveError),
    #[error("{message}")]
    Lower { span: Span, message: String },
}

impl IrError {
    pub fn display(&self, source: &Source) -> String {
        match self {
            Self::Parse(err) => render(source, err.span, "error", &err.message),
            Self::Resolve(err) => {
                let src = err.origin.as_ref().unwrap_or(source);
                render(src, err.span, "error", &err.message)
            }
            Self::Lower { span, message } => render(source, *span, "error", message),
        }
    }

    pub fn span(&self) -> Option<Span> {
        match self {
            Self::Parse(err) => Some(err.span),
            Self::Resolve(err) => Some(err.span),
            Self::Lower { span, .. } => Some(*span),
        }
    }
}
