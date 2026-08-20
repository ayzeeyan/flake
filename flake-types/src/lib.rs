//! Type system, effect tracking, and gradual ownership for Flake.

mod check;
mod effects;
mod error;
mod ownership;
mod ty;

pub use check::{check, check_program, check_str};
pub use effects::{Effect, EffectSet};
pub use error::{CheckError, TypeError};
pub use ownership::check_ownership;
pub use ty::Type;

/// Current crate version, matching the workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests;
