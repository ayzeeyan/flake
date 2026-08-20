//! Abstract syntax tree and source spans for Flake.
//!
//! The full AST lands in Milestone 2. Spans and source maps live here so the
//! lexer, parser, and later stages share one representation.

mod span;

pub use span::{LineCol, Source, Span};

/// Current crate version, matching the workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_semver() {
        assert!(!version().is_empty());
        assert!(version().contains('.'));
    }
}
