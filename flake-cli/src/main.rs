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
        #[arg(long, conflicts_with = "native")]
        vm: bool,
        /// Compile to native x86-64 and run the executable
        #[arg(long, conflicts_with = "vm")]
        native: bool,
    },
    /// Compile a Flake program to a native x86-64 executable
    Build {
        /// Path to a `.flk` source file
        file: PathBuf,
        /// Output path (default: `<stem>.exe`)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Also emit a human-readable `.s` assembly listing
        #[arg(long)]
        emit_asm: bool,
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
        Commands::Build {
            file,
            output,
            emit_asm,
        } => build_file(&file, output, emit_asm),
        Commands::Check { file } => check_file(&file),
        Commands::Ir { file } => dump_ir(&file),
        Commands::Repl => {
            let code = repl::run();
            ExitCode::from(code as u8)
        }
    }
}

fn emit_check_error(entry: &Source, err: &flake_types::CheckError) {
    match err {
        flake_types::CheckError::Parse(e) => report::emit(entry, e.span, &e.message),
        flake_types::CheckError::Type(e) => report::emit(entry, e.span, &e.message),
        flake_types::CheckError::TypeIn { origin, error } => {
            report::emit(origin, error.span, &error.message);
        }
        flake_types::CheckError::Resolve(e) => {
            let src = e.origin.as_ref().unwrap_or(entry);
            report::emit(src, e.span, &e.message);
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
            emit_check_error(&source, &err);
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

fn build_file(path: &PathBuf, output: Option<PathBuf>, emit_asm: bool) -> ExitCode {
    let source = match load_source(path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    if let Err(err) = check(&source) {
        emit_check_error(&source, &err);
        return ExitCode::from(1);
    }
    let out = output.unwrap_or_else(|| {
        let mut p = path.clone();
        p.set_extension("exe");
        p
    });
    let asm_out = out.with_extension("s");
    let result = if emit_asm {
        flake_codegen::write_executable_with_asm(&source, &out, &asm_out)
    } else {
        flake_codegen::write_executable(&source, &out)
    };
    match result {
        Ok(()) => {
            println!("wrote {}", out.display());
            if emit_asm {
                println!("wrote {}", asm_out.display());
            }
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
            emit_check_error(&source, &err);
            ExitCode::from(1)
        }
    }
}
