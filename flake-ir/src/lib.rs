//! Custom intermediate representation for Flake.
//!
//! Pipeline: source → AST → **IR** → bytecode VM or native x86-64.

mod error;
mod ir;
mod lower;
mod pretty;
mod ty;

pub use error::IrError;
pub use ir::*;
pub use lower::{lower, lower_program};
pub use pretty::print_module;
pub use ty::IrType;

/// Current crate version, matching the workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests;
