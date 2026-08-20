//! Abstract syntax tree and source spans for Flake.
//!
//! The full AST lands in Milestone 2. This crate currently exports version
//! metadata and a placeholder module layout so the workspace compiles.

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
