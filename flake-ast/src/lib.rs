//! Abstract syntax tree and source spans for Flake.

mod ast;
mod pretty;
mod report;
mod span;

pub use ast::*;
pub use pretty::print_program;
pub use report::render;
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
