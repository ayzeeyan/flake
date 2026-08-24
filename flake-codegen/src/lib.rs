pub mod aarch64;
mod emit;
pub mod elf;
mod error;
mod pe;
mod regalloc;
mod runtime;
pub mod target;
mod x86;

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use flake_ast::Source;
use flake_ir::lower;

pub use aarch64::compile_module_aarch64;
pub use elf::write_elf;
pub use emit::compile_module;
pub use error::CodegenError;
pub use pe::write_pe;
pub use target::{Target, TargetArch, TargetOs};

static UNIQUE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct PendingFile {
    path: Option<PathBuf>,
}

impl PendingFile {
    fn stage(target: &Path, contents: &[u8]) -> Result<Self, CodegenError> {
        let name = target.file_name().ok_or_else(|| {
            CodegenError::new(format!(
                "output path has no file name: {}",
                target.display()
            ))
        })?;
        let parent = target
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));

        for _ in 0..32 {
            let id = UNIQUE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let candidate = parent.join(format!(
                ".{}.flake-tmp-{}-{id}",
                name.to_string_lossy(),
                std::process::id()
            ));
            let mut file = match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => file,
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    return Err(CodegenError::new(format!(
                        "cannot create output beside {}: {err}",
                        target.display()
                    )));
                }
            };
            let pending = Self {
                path: Some(candidate),
            };
            if let Err(err) = file.write_all(contents) {
                drop(file);
                return Err(CodegenError::new(format!(
                    "cannot write {}: {err}",
                    target.display()
                )));
            }
            if let Err(err) = file.sync_all() {
                drop(file);
                return Err(CodegenError::new(format!(
                    "cannot flush {}: {err}",
                    target.display()
                )));
            }
            drop(file);
            return Ok(pending);
        }
        Err(CodegenError::new(format!(
            "cannot reserve a temporary output for {}",
            target.display()
        )))
    }

    fn persist(mut self, target: &Path) -> Result<(), CodegenError> {
        let staged = self.path.as_ref().expect("staged output path").clone();
        match fs::rename(&staged, target) {
            Ok(()) => {
                self.path = None;
                return Ok(());
            }
            Err(err) if !target.exists() => {
                return Err(CodegenError::new(format!(
                    "cannot install {}: {err}",
                    target.display()
                )));
            }
            Err(_) => {}
        }

        if fs::symlink_metadata(target)
            .map_err(|err| {
                CodegenError::new(format!("cannot inspect {}: {err}", target.display()))
            })?
            .is_dir()
        {
            return Err(CodegenError::new(format!(
                "output path is a directory: {}",
                target.display()
            )));
        }

        let backup = unused_sibling(target, "old")?;
        fs::rename(target, &backup).map_err(|err| {
            CodegenError::new(format!("cannot replace {}: {err}", target.display()))
        })?;
        match fs::rename(&staged, target) {
            Ok(()) => {
                self.path = None;
                fs::remove_file(&backup).map_err(|err| {
                    CodegenError::new(format!(
                        "wrote {}, but could not remove its backup {}: {err}",
                        target.display(),
                        backup.display()
                    ))
                })?;
                Ok(())
            }
            Err(err) => {
                let restore = fs::rename(&backup, target);
                if let Err(restore_err) = restore {
                    return Err(CodegenError::new(format!(
                        "cannot install {} ({err}) or restore its backup {} ({restore_err})",
                        target.display(),
                        backup.display()
                    )));
                }
                Err(CodegenError::new(format!(
                    "cannot install {}: {err}",
                    target.display()
                )))
            }
        }
    }
}

impl Drop for PendingFile {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

fn unused_sibling(target: &Path, role: &str) -> Result<PathBuf, CodegenError> {
    let name = target.file_name().ok_or_else(|| {
        CodegenError::new(format!(
            "output path has no file name: {}",
            target.display()
        ))
    })?;
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    for _ in 0..32 {
        let id = UNIQUE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{}.flake-{role}-{}-{id}",
            name.to_string_lossy(),
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(CodegenError::new(format!(
        "cannot reserve a backup path for {}",
        target.display()
    )))
}

fn write_file_atomically(target: &Path, contents: &[u8]) -> Result<(), CodegenError> {
    PendingFile::stage(target, contents)?.persist(target)
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Compile Flake source to binary executable bytes for the specified target.
pub fn compile_target(source: &Source, target: Target) -> Result<Vec<u8>, CodegenError> {
    let module = lower(source).map_err(|e| CodegenError::new(e.to_string()))?;
    compile_target_module(&module, target)
}

/// Compile an IR module to binary executable bytes for the specified target.
pub fn compile_target_module(
    module: &flake_ir::Module,
    target: Target,
) -> Result<Vec<u8>, CodegenError> {
    match (target.arch, target.os) {
        (TargetArch::X86_64, TargetOs::Windows) => {
            let compiled = compile_module(module)?;
            Ok(write_pe(&compiled))
        }
        (TargetArch::X86_64, TargetOs::Linux) => {
            let compiled = compile_module(module)?;
            Ok(write_elf(&compiled, TargetArch::X86_64))
        }
        (TargetArch::Aarch64, TargetOs::Linux | TargetOs::Windows) => {
            let compiled = compile_module_aarch64(module)?;
            Ok(write_elf(&compiled, TargetArch::Aarch64))
        }
    }
}

/// Compile Flake source to a Windows PE x86-64 executable.
pub fn compile_exe(source: &Source) -> Result<Vec<u8>, CodegenError> {
    compile_target(source, Target::X86_64_WINDOWS)
}

/// Compile Flake source to a Linux ELF64 executable.
pub fn compile_elf(source: &Source, arch: TargetArch) -> Result<Vec<u8>, CodegenError> {
    compile_target(
        source,
        Target {
            arch,
            os: TargetOs::Linux,
        },
    )
}

/// Compile to GNU-style assembly text for inspection and diagnostics.
pub fn compile_asm(source: &Source) -> Result<String, CodegenError> {
    let module = lower(source).map_err(|e| CodegenError::new(e.to_string()))?;
    let compiled = compile_module(&module)?;
    Ok(compiled.gas)
}

/// Write a native executable to `out` for the given target.
pub fn write_executable_for_target(
    source: &Source,
    out: &Path,
    target: Target,
) -> Result<(), CodegenError> {
    let bytes = compile_target(source, target)?;
    write_file_atomically(out, &bytes)
}

/// Write a native executable to `out` without auxiliary artifacts.
pub fn write_executable(source: &Source, out: &Path) -> Result<(), CodegenError> {
    write_executable_for_target(source, out, Target::default())
}

/// Write a native executable and an explicitly requested assembly listing for target.
pub fn write_executable_with_asm_for_target(
    source: &Source,
    out: &Path,
    asm_out: &Path,
    target: Target,
) -> Result<(), CodegenError> {
    if out == asm_out {
        return Err(CodegenError::new(
            "executable and assembly output paths must be different",
        ));
    }
    let module = lower(source).map_err(|e| CodegenError::new(e.to_string()))?;
    let (bytes, gas) = match target.arch {
        TargetArch::X86_64 => {
            let compiled = compile_module(&module)?;
            let b = match target.os {
                TargetOs::Windows => write_pe(&compiled),
                TargetOs::Linux => write_elf(&compiled, TargetArch::X86_64),
            };
            (b, compiled.gas)
        }
        TargetArch::Aarch64 => {
            let compiled = compile_module_aarch64(&module)?;
            let b = write_elf(&compiled, TargetArch::Aarch64);
            (b, compiled.gas)
        }
    };
    write_file_atomically(asm_out, gas.as_bytes())?;
    write_file_atomically(out, &bytes)
}

/// Write a native executable and an explicitly requested assembly listing.
pub fn write_executable_with_asm(
    source: &Source,
    out: &Path,
    asm_out: &Path,
) -> Result<(), CodegenError> {
    write_executable_with_asm_for_target(source, out, asm_out, Target::default())
}

/// Compile, write a temp exe, run it, return stdout.
pub fn run_native(source: &Source) -> Result<String, CodegenError> {
    let dir = std::env::temp_dir();
    let n = UNIQUE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let exe = dir.join(format!("flake-native-{}-{n}.exe", std::process::id()));
    write_executable(source, &exe)?;
    let _remove_exe = RemoveOnDrop(exe.clone());
    let output = Command::new(&exe)
        .output()
        .map_err(|e| CodegenError::new(format!("failed to run native binary: {e}")))?;
    if !output.status.success() {
        let mut message = format!("native process exited with {:?}", output.status.code());
        if !output.stdout.is_empty() {
            message.push_str("\nstdout:\n");
            message.push_str(&String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            message.push_str("\nstderr:\n");
            message.push_str(&String::from_utf8_lossy(&output.stderr));
        }
        return Err(CodegenError::new(message));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Current crate version.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests;
