//! `flake` — the command-line interface for the Flake programming language.

mod repl;
mod report;
mod bootstrap;

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use flake_ast::Source;
use flake_interpreter::execute;
use flake_types::check;

/// Flake — a safe, modern systems language with gradual ownership and first-class effects.
#[derive(Debug, Parser)]
#[command(
    name = "flake",
    version = "1.0.0",
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
        /// Arguments forwarded to the Flake program (`args()`). Pass after `--`.
        #[arg(last = true)]
        program_args: Vec<String>,
    },
    /// Compile a Flake program or local package to a native executable
    Build {
        /// Path to a `.flk` source file or package directory (default: current package)
        file: Option<PathBuf>,
        /// Output path (default: `<stem>.exe` or `<stem>`)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Target triple (e.g. x86_64-windows, x86_64-linux, aarch64-linux)
        #[arg(long)]
        target: Option<String>,
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
    /// Generate or check `flake.lock` for the current package or workspace
    Lock {
        /// Path to a package directory or `flake.toml` (default: current package)
        file: Option<PathBuf>,
        /// Verify that `flake.lock` is up-to-date without modifying it
        #[arg(long)]
        check: bool,
    },
    /// Update dependencies in `flake.lock`
    Update {
        /// Path to a package directory or `flake.toml` (default: current package)
        file: Option<PathBuf>,
    },
    /// Start an interactive Flake REPL
    Repl,
    /// Run the bootstrap cycle (Stage 0 build, Stage 1 self-check, Stage 2 rebuild & verify)
    Bootstrap {
        /// Target triple (default: host target)
        #[arg(long)]
        target: Option<String>,
        /// Keep intermediate build binaries and artifacts in target/bootstrap/
        #[arg(long)]
        keep: bool,
        /// Verbose diagnostic output
        #[arg(short, long)]
        verbose: bool,
    },
}

fn main() -> ExitCode {
    const STACK_SIZE: usize = 16 * 1024 * 1024;
    std::thread::Builder::new()
        .name("flake-main".into())
        .stack_size(STACK_SIZE)
        .spawn(run_cli)
        .expect("failed to spawn main thread")
        .join()
        .unwrap_or(ExitCode::from(1))
}

fn run_cli() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            file,
            skip_check,
            vm,
            native,
            program_args,
        } => match resolve_target_path(file.as_ref()) {
            Ok(target) => run_file(&target, skip_check, vm, native, &program_args),
            Err(code) => code,
        },
        Commands::Build {
            file,
            output,
            target,
            emit_asm,
        } => match resolve_target_path(file.as_ref()) {
            Ok(target_path) => build_file(&target_path, output, target, emit_asm),
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
        Commands::Lock { file, check } => lock_package(file.as_ref(), check),
        Commands::Update { file } => update_package(file.as_ref()),
        Commands::Repl => {
            let code = repl::run();
            ExitCode::from(code as u8)
        }
        Commands::Bootstrap {
            target,
            keep,
            verbose,
        } => bootstrap::run_bootstrap(target, keep, verbose),
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
        report::emit_message(
            "no input file specified and no flake.toml found in current directory",
        );
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
        let main_content = format!("fn main() / io {{\n    print(\"Hello from {name}!\")\n}}\n");
        if let Err(e) = fs::write(&main_path, main_content) {
            report::emit_message(&format!("failed to write {}: {e}", main_path.display()));
            return ExitCode::from(1);
        }
    }
    println!("Initialized package `{name}` in {}", cur_dir.display());
    ExitCode::SUCCESS
}

fn new_package(path: &PathBuf) -> ExitCode {
    if path.exists()
        && fs::read_dir(path)
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        report::emit_message(&format!(
            "destination `{}` already exists and is not empty",
            path.display()
        ));
        return ExitCode::from(1);
    }
    if let Err(e) = fs::create_dir_all(path) {
        report::emit_message(&format!(
            "failed to create directory `{}`: {e}",
            path.display()
        ));
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
    let main_content = format!("fn main() / io {{\n    print(\"Hello from {name}!\")\n}}\n");
    if let Err(e) = fs::write(&main_path, main_content) {
        report::emit_message(&format!("failed to write {}: {e}", main_path.display()));
        return ExitCode::from(1);
    }
    println!("Created package `{name}` at {}", path.display());
    ExitCode::SUCCESS
}

fn lock_package(file: Option<&PathBuf>, check_only: bool) -> ExitCode {
    let start_dir = file.cloned().unwrap_or_else(|| PathBuf::from("."));
    let Some((dir, manifest)) = flake_parser::Manifest::find_in_ancestors(&start_dir) else {
        report::emit_message("no flake.toml manifest found to lock");
        return ExitCode::from(1);
    };
    let lock_path = dir.join("flake.lock");

    if check_only {
        if !lock_path.is_file() {
            report::emit_message(&format!(
                "lockfile {} does not exist (run `flake lock`)",
                lock_path.display()
            ));
            return ExitCode::from(1);
        }
        let content = match fs::read_to_string(&lock_path) {
            Ok(c) => c,
            Err(e) => {
                report::emit_message(&format!("failed to read {}: {e}", lock_path.display()));
                return ExitCode::from(1);
            }
        };
        let lockfile = match flake_parser::Lockfile::parse(&content, &lock_path) {
            Ok(l) => l,
            Err(e) => {
                report::emit_message(&format!(
                    "invalid lockfile {}: {}",
                    lock_path.display(),
                    e.message
                ));
                return ExitCode::from(1);
            }
        };
        if let Err(e) = lockfile.verify(&manifest, &dir) {
            report::emit_message(&format!("lockfile check failed: {}", e.message));
            return ExitCode::from(1);
        }
        println!("Lockfile {} is up to date", lock_path.display());
        return ExitCode::SUCCESS;
    }

    let lockfile = match flake_parser::Lockfile::generate(&manifest, &dir) {
        Ok(l) => l,
        Err(e) => {
            report::emit_message(&format!("failed to generate lockfile: {}", e.message));
            return ExitCode::from(1);
        }
    };
    let toml = lockfile.to_toml_string();
    if let Err(e) = fs::write(&lock_path, toml) {
        report::emit_message(&format!("failed to write {}: {e}", lock_path.display()));
        return ExitCode::from(1);
    }
    println!(
        "Locked {} packages to {}",
        lockfile.packages.len(),
        lock_path.display()
    );
    ExitCode::SUCCESS
}

fn update_package(file: Option<&PathBuf>) -> ExitCode {
    let start_dir = file.cloned().unwrap_or_else(|| PathBuf::from("."));
    let Some((dir, manifest)) = flake_parser::Manifest::find_in_ancestors(&start_dir) else {
        report::emit_message("no flake.toml manifest found to update");
        return ExitCode::from(1);
    };
    let lock_path = dir.join("flake.lock");
    let lockfile = match flake_parser::Lockfile::generate(&manifest, &dir) {
        Ok(l) => l,
        Err(e) => {
            report::emit_message(&format!("failed to update lockfile: {}", e.message));
            return ExitCode::from(1);
        }
    };
    let toml = lockfile.to_toml_string();
    if let Err(e) = fs::write(&lock_path, toml) {
        report::emit_message(&format!("failed to write {}: {e}", lock_path.display()));
        return ExitCode::from(1);
    }
    println!(
        "Updated {} ({} packages locked)",
        lock_path.display(),
        lockfile.packages.len()
    );
    ExitCode::SUCCESS
}

fn verify_lockfile_if_present(target_path: &Path) -> Result<(), ExitCode> {
    if let Some((dir, manifest)) = flake_parser::Manifest::find_in_ancestors(target_path) {
        let lock_path = dir.join("flake.lock");
        if lock_path.is_file() {
            let content = match fs::read_to_string(&lock_path) {
                Ok(c) => c,
                Err(e) => {
                    report::emit_message(&format!("failed to read {}: {e}", lock_path.display()));
                    return Err(ExitCode::from(1));
                }
            };
            let lockfile = match flake_parser::Lockfile::parse(&content, &lock_path) {
                Ok(l) => l,
                Err(e) => {
                    report::emit_message(&format!(
                        "invalid lockfile {}: {}",
                        lock_path.display(),
                        e.message
                    ));
                    return Err(ExitCode::from(1));
                }
            };
            if let Err(e) = lockfile.verify(&manifest, &dir) {
                report::emit_message(&format!("{}: {}", lock_path.display(), e.message));
                return Err(ExitCode::from(1));
            }
        }
    }
    Ok(())
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

fn run_file(
    path: &PathBuf,
    skip_check: bool,
    use_vm: bool,
    native: bool,
    program_args: &[String],
) -> ExitCode {
    if !skip_check {
        if let Err(code) = verify_lockfile_if_present(path) {
            return code;
        }
    }
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
    flake_interpreter::set_program_args(program_args.to_vec());
    flake_vm::set_program_args(program_args.to_vec());
    if native {
        match flake_codegen::run_native_with_args(&source, program_args) {
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

fn build_file(
    path: &PathBuf,
    output: Option<PathBuf>,
    target_str: Option<String>,
    emit_asm: bool,
) -> ExitCode {
    if let Err(code) = verify_lockfile_if_present(path) {
        return code;
    }
    let source = match load_source(path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    if let Err(err) = check(&source) {
        emit_check_error(&source, &err);
        return ExitCode::from(1);
    }
    let target = match target_str {
        Some(s) => match s.parse::<flake_codegen::Target>() {
            Ok(t) => t,
            Err(err) => {
                report::emit_message(&err.to_string());
                return ExitCode::from(1);
            }
        },
        None => {
            if let Some(ref out) = output {
                if out.extension().and_then(|e| e.to_str()) == Some("exe") {
                    flake_codegen::Target::X86_64_WINDOWS
                } else {
                    flake_codegen::Target::default()
                }
            } else {
                flake_codegen::Target::default()
            }
        }
    };
    let out = output.unwrap_or_else(|| {
        let mut p = path.clone();
        let ext = target.default_extension();
        p.set_extension(ext);
        p
    });
    let asm_out = out.with_extension("s");
    let result = if emit_asm {
        flake_codegen::write_executable_with_asm_for_target(&source, &out, &asm_out, target)
    } else {
        flake_codegen::write_executable_for_target(&source, &out, target)
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
    if let Err(code) = verify_lockfile_if_present(path) {
        return code;
    }
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
