//! `flake` — the command-line interface for the Flake programming language.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

const TAGLINE: &str = "Clarity, crystallized.";

/// Flake — a safe, modern systems language with gradual ownership and first-class effects.
#[derive(Debug, Parser)]
#[command(
    name = "flake",
    version,
    about = "Flake — Clarity, crystallized.",
    long_about = "Flake is a safe, modern systems language with gradual ownership \
and a first-class effect system that makes side effects visible and controllable.\n\n\
Clarity, crystallized.",
    propagate_version = true,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run a Flake program
    Run {
        /// Path to a `.flk` source file
        file: PathBuf,
    },
    /// Type-check a Flake program without running it
    Check {
        /// Path to a `.flk` source file
        file: PathBuf,
    },
    /// Start an interactive Flake REPL
    Repl,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run { file } => not_yet("run", Some(&file)),
        Commands::Check { file } => not_yet("check", Some(&file)),
        Commands::Repl => not_yet("repl", None),
    }
}

fn not_yet(command: &str, file: Option<&PathBuf>) -> ExitCode {
    eprint!("flake {command}: not implemented yet");
    if let Some(path) = file {
        eprint!(" (target: {})", path.display());
    }
    eprintln!();
    eprintln!("{TAGLINE}");
    eprintln!("See ROADMAP.md for milestone status.");
    ExitCode::from(2)
}
