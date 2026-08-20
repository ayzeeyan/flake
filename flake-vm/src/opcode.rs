//! Instruction set for the Flake bytecode VM.

use crate::value::Value;

/// A compiled function body.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub ops: Vec<Op>,
    pub constants: Vec<Value>,
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            constants: Vec::new(),
        }
    }

    pub fn emit(&mut self, op: Op) -> usize {
        let i = self.ops.len();
        self.ops.push(op);
        i
    }

    pub fn add_constant(&mut self, value: Value) -> u16 {
        if let Some(i) = self.constants.iter().position(|c| c == &value) {
            return i as u16;
        }
        let i = self.constants.len();
        self.constants.push(value);
        i as u16
    }

    pub fn patch_jump(&mut self, at: usize, target: usize) {
        match &mut self.ops[at] {
            Op::Jump(offset) | Op::JumpIfFalse(offset) => {
                *offset = target as u16;
            }
            _ => {}
        }
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

/// Stack-based opcodes. Jumps store absolute instruction indices.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Constant(u16),
    Nil,
    True,
    False,
    Pop,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Not,
    Neg,
    Jump(u16),
    JumpIfFalse(u16),
    GetLocal(u16),
    SetLocal(u16),
    GetGlobal(u16),
    DefineGlobal(u16),
    Call(u8),
    Return,
    Print,
    BuildList(u16),
    GetIndex,
    SetIndex,
    Concat(u8),
}
