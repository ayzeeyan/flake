//! Lexer for Flake source (`.flk` files).
//!
//! Turns source text into a token stream: keywords, identifiers, literals,
//! operators, string interpolation markers, and comments (which are discarded).

mod error;
mod lexer;
mod token;

pub use error::LexError;
pub use lexer::{dump_tokens, tokenize, tokenize_str};
pub use token::{Token, TokenKind};

/// Current crate version, matching the workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests;
