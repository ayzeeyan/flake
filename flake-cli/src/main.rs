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
    /// Run a Flake program or local package
    Run {
        /// Path to a `.flk` source file or package directory (default: current package)
        file: Option<PathBuf>,
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
    /// Compile a Flake program or local package to a native x86-64 executable
    Build {
        /// Path to a `.flk` source file or package directory (default: current package)
        file: Option<PathBuf>,
        /// Output path (default: `<stem>.exe`)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Also emit a human-readable `.s` assembly listing
        #[arg(long)]
        emit_asm: bool,
    },
    /// Type-check a Flake program or local package without running it
    Check {
        /// Path to a `.flk` source file or package directory (default: current package)
        file: Option<PathBuf>,
    },
    /// Dump Flake IR for a program or local package
    Ir {
        /// Path to a `.flk` source file or package directory (default: current package)
        file: Option<PathBuf>,
    },
    /// Initialize a new Flake package in the current directory
    Init {
        /// Package name (default: current directory name)
        #[arg(long)]
        name: Option<String>,
    },
    /// Create a new Flake package directory
    New {
        /// Path for the new package directory
        path: PathBuf,
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
        } => match resolve_target_path(file.as_ref()) {
            Ok(target) => run_file(&target, skip_check, vm, native),
            Err(code) => code,
        },
        Commands::Build {
            file,
            output,
            emit_asm,
        } => match resolve_target_path(file.as_ref()) {
            Ok(target) => build_file(&target, output, emit_asm),
            Err(code) => code,
        },
        Commands::Check { file } => match resolve_target_path(file.as_ref()) {
            Ok(target) => check_file(&target),
            Err(code) => code,
        },
        Commands::Ir { file } => match resolve_target_path(file.as_ref()) {
            Ok(target) => dump_ir(&target),
            Err(code) => code,
        },
        Commands::Init { name } => init_package(name),
        Commands::New { path } => new_package(&path),
        Commands::Repl => {
            let code = repl::run();
            ExitCode::from(code as u8)
        }
    }
}

fn resolve_target_path(path: Option<&PathBuf>) -> Result<PathBuf, ExitCode> {
    if let Some(path) = path {
        if path.is_dir() {
            if let Some((dir, manifest)) = flake_parser::Manifest::find_in_ancestors(path) {
                return Ok(dir.join(&manifest.package.entry));
            }
            let default_main = path.join("main.flk");
            if default_main.is_file() {
                return Ok(default_main);
            }
            report::emit_message(&format!(
                "no flake.toml or main.flk found in {}",
                path.display()
            ));
            return Err(ExitCode::from(1));
        }
        if path.file_name().and_then(|n| n.to_str()) == Some("flake.toml") {
            if let Some((dir, manifest)) = flake_parser::Manifest::find_in_ancestors(path) {
                return Ok(dir.join(&manifest.package.entry));
            }
        }
        Ok(path.clone())
    } else {
        let cur = PathBuf::from(".");
        if let Some((dir, manifest)) = flake_parser::Manifest::find_in_ancestors(&cur) {
            return Ok(dir.join(&manifest.package.entry));
        }
        let default_main = PathBuf::from("main.flk");
        if default_main.is_file() {
            return Ok(default_main);
        }
        report::emit_message("no input file specified and no flake.toml found in current directory");
        Err(ExitCode::from(1))
    }
}

fn init_package(name_override: Option<String>) -> ExitCode {
    let cur_dir = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            report::emit_message(&format!("failed to get current directory: {e}"));
            return ExitCode::from(1);
        }
    };
    let name = name_override.unwrap_or_else(|| {
        cur_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("app")
            .to_string()
    });
    let manifest_path = cur_dir.join("flake.toml");
    if manifest_path.exists() {
        report::emit_message(&format!(
            "package manifest already exists at {}",
            manifest_path.display()
        ));
        return ExitCode::from(1);
    }
    let manifest_content = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
entry = "main.flk"

[dependencies]
"#
    );
    if let Err(e) = fs::write(&manifest_path, manifest_content) {
        report::emit_message(&format!("failed to write {}: {e}", manifest_path.display()));
        return ExitCode::from(1);
    }
    let main_path = cur_dir.join("main.flk");
    if !main_path.exists() {
        let main_content = format!(
            "fn main() / io {{\n    print(\"Hello from {name}!\")\n}}\n"
        );
        if let Err(e) = fs::write(&main_path, main_content) {
            report::emit_message(&format!("failed to write {}: {e}", main_path.display()));
            return ExitCode::from(1);
        }
    }
    println!("Initialized package `{name}` in {}", cur_dir.display());
    ExitCode::SUCCESS
}

fn new_package(path: &PathBuf) -> ExitCode {
    if path.exists() && fs::read_dir(path).map(|mut d| d.next().is_some()).unwrap_or(false) {
        report::emit_message(&format!(
            "destination `{}` already exists and is not empty",
            path.display()
        ));
        return ExitCode::from(1);
    }
    if let Err(e) = fs::create_dir_all(path) {
        report::emit_message(&format!("failed to create directory `{}`: {e}", path.display()));
        return ExitCode::from(1);
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("app")
        .to_string();
    let manifest_path = path.join("flake.toml");
    let manifest_content = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
entry = "main.flk"

[dependencies]
"#
    );
    if let Err(e) = fs::write(&manifest_path, manifest_content) {
        report::emit_message(&format!("failed to write {}: {e}", manifest_path.display()));
        return ExitCode::from(1);
    }
    let main_path = path.join("main.flk");
    let main_content = format!(
        "fn main() / io {{\n    print(\"Hello from {name}!\")\n}}\n"
    );
    if let Err(e) = fs::write(&main_path, main_content) {
        report::emit_message(&format!("failed to write {}: {e}", main_path.display()));
        return ExitCode::from(1);
    }
    println!("Created package `{name}` at {}", path.display());
    ExitCode::SUCCESS
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
