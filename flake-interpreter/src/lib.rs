//! Tree-walking interpreter for Flake.

mod env;
mod error;
mod eval;
mod value;

pub use error::{RunError, RuntimeError};
pub use eval::{execute, execute_captured, execute_program};
pub use value::{Function, NativeFn, Value};

/// Current crate version, matching the workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests;
