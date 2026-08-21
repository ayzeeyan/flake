//! Recursive-descent parser for Flake.

mod error;
mod parser;
mod resolve;

pub use error::ParseError;
pub use parser::{parse, parse_repl, parse_str, ReplInput};
pub use resolve::{
    import_alias, load_graph, qualify, LoadedModule, ModuleGraph, ResolveError,
};

/// Current crate version, matching the workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests;
