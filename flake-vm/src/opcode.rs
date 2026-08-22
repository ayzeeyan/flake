//! Instruction set for the Flake bytecode VM.

use flake_ast::Span;

use crate::value::Value;

/// A compiled function body.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub ops: Vec<Op>,
    pub constants: Vec<Value>,
    pub spans: Vec<Span>,
    current_span: Span,
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            constants: Vec::new(),
            spans: Vec::new(),
            current_span: Span::DUMMY,
        }
    }

    pub fn emit(&mut self, op: Op) -> usize {
        let i = self.ops.len();
        self.ops.push(op);
        self.spans.push(self.current_span);
        i
    }

    pub fn replace_span(&mut self, span: Span) -> Span {
        std::mem::replace(&mut self.current_span, span)
    }

    pub fn set_span(&mut self, span: Span) {
        self.current_span = span;
    }

    pub fn add_constant(&mut self, value: Value) -> u16 {
        if let Some(i) = self.constants.iter().position(|c| {
            matches!(
                c,
                Value::Nil | Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::String(_)
            ) && c == &value
        }) {
            return i as u16;
        }
        let i = self.constants.len();
        self.constants.push(value);
        i as u16
    }

    pub fn patch_jump(&mut self, at: usize, target: usize) {
        match &mut self.ops[at] {
            Op::Jump(offset) | Op::JumpIfFalse(offset) | Op::IterNext(offset) => {
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
    Dup,
    DupTwo,
    Swap,
    Rot3,
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
    /// Capture a callee and arguments in a scope-bound cooperative task.
    Spawn(u8),
    /// Wrap an already-computed value (used for enum constructors).
    ReadyTask,
    /// Join a task and replace it with its result.
    Await,
    Return,
    Print,
    BuildList(u16),
    BuildMap(u16),
    BuildStruct {
        name: u16,
        fields: Vec<u16>,
    },
    MakeRange,
    MakeIter,
    IterNext(u16),
    GetIndex,
    SetIndex,
    GetField(u16),
    SetField(u16),
    Concat(u8),
}
