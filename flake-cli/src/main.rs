//! `flake` — the command-line interface for the Flake programming language.

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use flake_ast::Source;
use flake_interpreter::execute;
use flake_types::check;

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
        /// Skip the type checker and run anyway
        #[arg(long)]
        skip_check: bool,
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
        Commands::Run { file, skip_check } => run_file(&file, skip_check),
        Commands::Check { file } => check_file(&file),
        Commands::Repl => not_yet("repl", None),
    }
}

fn load_source(path: &PathBuf) -> Result<Source, ExitCode> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Source::new(path.display().to_string(), text)),
        Err(err) => {
            eprintln!("error: cannot read {}: {err}", path.display());
            Err(ExitCode::from(1))
        }
    }
}

fn run_file(path: &PathBuf, skip_check: bool) -> ExitCode {
    let source = match load_source(path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    if !skip_check {
        if let Err(err) = check(&source) {
            eprint!("{}", err.display(&source));
            return ExitCode::from(1);
        }
    }
    let mut stdout = io::stdout();
    match execute(&source, &mut stdout) {
        Ok(_) => {
            let _ = stdout.flush();
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprint!("{}", err.display(&source));
            ExitCode::from(1)
        }
    }
}

fn check_file(path: &PathBuf) -> ExitCode {
    let source = match load_source(path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    match check(&source) {
        Ok(_) => {
            println!("ok");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprint!("{}", err.display(&source));
            ExitCode::from(1)
        }
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
