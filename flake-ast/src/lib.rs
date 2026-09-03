//! Abstract syntax tree and source spans for Flake.

mod ast;
mod ctfe;
mod pretty;
mod report;
mod span;

pub use ast::*;
pub use ctfe::{
    ConstError, ConstValue, collect_const_values, eval_const_expr, eval_const_expr_with_fns,
};
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
