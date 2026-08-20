//! Stack-based bytecode virtual machine for Flake.
//!
//! The VM lands in Milestone 8. This crate currently exports version metadata
//! so the workspace compiles.

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
