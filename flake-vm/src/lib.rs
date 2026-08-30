//! Stack-based bytecode virtual machine for Flake.

mod compiler;
mod error;
mod natives;
mod opcode;
mod value;
mod vm;

use std::io::Write;

use flake_ast::Source;
use flake_parser::load_graph;

pub use error::{ExecuteError, VmError};
pub use natives::set_program_args;
pub use opcode::{Chunk, Op};
pub use value::{Function, Native, Value};
pub use vm::Vm;

use compiler::compile_graph;

/// Parse, compile, and execute `source` on the bytecode VM.
pub fn execute(source: &Source, stdout: &mut dyn Write) -> Result<Value, ExecuteError> {
    let graph = load_graph(source)?;
    let compiled = compile_graph(&graph)?;
    let mut vm = Vm::new(stdout);
    for func in compiled.functions {
        vm.define_function(func);
    }
    vm.run_main().map_err(ExecuteError::Runtime)
}

/// Execute and capture stdout.
pub fn execute_captured(source: &Source) -> Result<(Value, String), ExecuteError> {
    let mut buf = Vec::new();
    let value = execute(source, &mut buf)?;
    Ok((value, String::from_utf8_lossy(&buf).into_owned()))
}

/// Current crate version, matching the workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests;
