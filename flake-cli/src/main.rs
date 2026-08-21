//! `flake` — the command-line interface for the Flake programming language.

mod repl;
mod report;

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use flake_ast::Source;
use flake_interpreter::execute;
use flake_types::check;

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
        /// Execute on the bytecode VM instead of the tree-walking interpreter
        #[arg(long)]
        vm: bool,
        /// Compile to native x86-64 and run the executable
        #[arg(long)]
        native: bool,
    },
    /// Compile a Flake program to a native x86-64 executable
    Build {
        /// Path to a `.flk` source file
        file: PathBuf,
        /// Output path (default: `<stem>.exe`)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Type-check a Flake program without running it
    Check {
        /// Path to a `.flk` source file
        file: PathBuf,
    },
    /// Dump Flake IR for a program
    Ir {
        /// Path to a `.flk` source file
        file: PathBuf,
    },
    /// Start an interactive Flake REPL
    Repl,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            file,
            skip_check,
            vm,
            native,
        } => run_file(&file, skip_check, vm, native),
        Commands::Build { file, output } => build_file(&file, output),
        Commands::Check { file } => check_file(&file),
        Commands::Ir { file } => dump_ir(&file),
        Commands::Repl => {
            let code = repl::run();
            ExitCode::from(code as u8)
        }
    }
}

fn load_source(path: &PathBuf) -> Result<Source, ExitCode> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Source::new(path.display().to_string(), text)),
        Err(err) => {
            report::emit_message(&format!("cannot read {}: {err}", path.display()));
            Err(ExitCode::from(1))
        }
    }
}

fn run_file(path: &PathBuf, skip_check: bool, use_vm: bool, native: bool) -> ExitCode {
    let source = match load_source(path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    if !skip_check {
        if let Err(err) = check(&source) {
            match &err {
                flake_types::CheckError::Parse(e) => report::emit(&source, e.span, &e.message),
                flake_types::CheckError::Type(e) => report::emit(&source, e.span, &e.message),
            }
            return ExitCode::from(1);
        }
    }
    if native {
        match flake_codegen::run_native(&source) {
            Ok(out) => {
                print!("{out}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                report::emit_message(&err.to_string());
                ExitCode::from(1)
            }
        }
    } else {
    let mut stdout = io::stdout();
    if use_vm {
        match flake_vm::execute(&source, &mut stdout) {
            Ok(_) => {
                let _ = stdout.flush();
                ExitCode::SUCCESS
            }
            Err(err) => {
                if let Some(span) = err.span() {
                    report::emit(&source, span, &err.to_string());
                } else {
                    report::emit_message(&err.to_string());
                }
                ExitCode::from(1)
            }
        }
    } else {
        match execute(&source, &mut stdout) {
            Ok(_) => {
                let _ = stdout.flush();
                ExitCode::SUCCESS
            }
            Err(err) => {
                if let Some(span) = err.span() {
                    report::emit(&source, span, &err.to_string());
                } else {
                    report::emit_message(&err.to_string());
                }
                ExitCode::from(1)
            }
        }
    }
    }
}

fn build_file(path: &PathBuf, output: Option<PathBuf>) -> ExitCode {
    let source = match load_source(path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let out = output.unwrap_or_else(|| {
        let mut p = path.clone();
        p.set_extension("exe");
        p
    });
    match flake_codegen::write_executable(&source, &out) {
        Ok(()) => {
            println!("wrote {}", out.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            report::emit_message(&err.to_string());
            ExitCode::from(1)
        }
    }
}

fn dump_ir(path: &PathBuf) -> ExitCode {
    let source = match load_source(path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    match flake_ir::lower(&source) {
        Ok(module) => {
            print!("{}", flake_ir::print_module(&module));
            ExitCode::SUCCESS
        }
        Err(err) => {
            if let Some(span) = err.span() {
                report::emit(&source, span, &err.to_string());
            } else {
                report::emit_message(&err.to_string());
            }
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
            match &err {
                flake_types::CheckError::Parse(e) => report::emit(&source, e.span, &e.message),
                flake_types::CheckError::Type(e) => report::emit(&source, e.span, &e.message),
            }
            ExitCode::from(1)
        }
    }
}
