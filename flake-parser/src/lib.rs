//! Recursive-descent parser for Flake.

mod error;
mod parser;

pub use error::ParseError;
pub use parser::{parse, parse_str};

/// Current crate version, matching the workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests;
