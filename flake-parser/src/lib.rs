//! Recursive-descent parser for Flake.

mod error;
mod parser;
mod resolve;

pub use error::ParseError;
pub use parser::{ReplInput, parse, parse_repl, parse_str};
pub use resolve::{
    LoadedModule, ModuleGraph, ResolveError, import_alias, is_exported, load_graph, qualify,
};

/// Current crate version, matching the workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests;
