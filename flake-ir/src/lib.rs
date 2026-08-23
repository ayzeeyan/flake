//! Custom intermediate representation for Flake.
//!
//! Native pipeline: source → AST → **IR** → x86-64 machine code.
//!
//! The interpreter and bytecode VM consume the AST directly.

mod error;
mod ir;
mod lower;
pub mod opt;
mod pretty;
mod ty;

pub use error::IrError;
pub use ir::*;
pub use lower::{lower, lower_program};
pub use opt::{optimize, optimize_function};
pub use pretty::print_module;
pub use ty::IrType;

/// Current crate version, matching the workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests;
