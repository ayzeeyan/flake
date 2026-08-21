//! Pure-Rust x86-64 code generator for Flake.
//!
//! Pipeline: AST → IR → machine code → PE32+ executable.
//! No LLVM, Cranelift, or C transpilation.

mod emit;
mod error;
mod pe;
mod regalloc;
mod runtime;
mod x86;

use std::fs;
use std::path::Path;
use std::process::Command;

use flake_ast::Source;
use flake_ir::lower;

pub use emit::compile_module;
pub use error::CodegenError;
pub use pe::write_pe;

/// Compile Flake source to a Windows PE x86-64 executable.
static UNIQUE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn compile_exe(source: &Source) -> Result<Vec<u8>, CodegenError> {
    let module = lower(source).map_err(|e| CodegenError::new(e.to_string()))?;
    let compiled = compile_module(&module)?;
    Ok(write_pe(&compiled))
}

/// Compile to GNU-style assembly text (for inspection or gcc/clang).
pub fn compile_asm(source: &Source) -> Result<String, CodegenError> {
    let module = lower(source).map_err(|e| CodegenError::new(e.to_string()))?;
    let compiled = compile_module(&module)?;
    Ok(compiled.gas)
}

/// Write an executable (and a `.s` sidecar) to `out`.
pub fn write_executable(source: &Source, out: &Path) -> Result<(), CodegenError> {
    let module = lower(source).map_err(|e| CodegenError::new(e.to_string()))?;
    let compiled = compile_module(&module)?;
    let asm_path = out.with_extension("s");
    fs::write(&asm_path, &compiled.gas).map_err(|e| CodegenError::new(e.to_string()))?;
    let pe = write_pe(&compiled);
    fs::write(out, pe).map_err(|e| CodegenError::new(e.to_string()))?;
    Ok(())
}

/// Compile, write a temp exe, run it, return stdout.
pub fn run_native(source: &Source) -> Result<String, CodegenError> {
    let dir = std::env::temp_dir();
    let n = UNIQUE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let exe = dir.join(format!("flake-native-{}-{n}.exe", std::process::id()));
    write_executable(source, &exe)?;
    let output = Command::new(&exe)
        .output()
        .map_err(|e| CodegenError::new(format!("failed to run native binary: {e}")))?;
    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(exe.with_extension("s"));
    if !output.status.success() {
        return Err(CodegenError::new(format!(
            "native process exited with {:?}\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Current crate version.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests;
