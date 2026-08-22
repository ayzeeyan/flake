//! Flake IR: a control-flow graph of basic blocks over local slots.
//!
//! Each function owns a list of locals (parameters first) and a list of
//! blocks. Instructions read and write locals. Control transfer is only
//! via the last instruction of a block (`Jump`, `Branch`, `Return`).
//!
//! This shape lowers cleanly to the bytecode VM and to x86-64: locals
//! become stack slots, blocks become labels.

use crate::ty::IrType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub functions: Vec<Function>,
    pub structs: Vec<StructDef>,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<(String, IrType)>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<LocalId>,
    pub ret: IrType,
    pub effects: Vec<String>,
    pub effects_specified: bool,
    pub strict: bool,
    pub owned: bool,
    pub locals: Vec<Local>,
    pub blocks: Vec<BasicBlock>,
    pub entry: BlockId,
}

#[derive(Debug, Clone)]
pub struct Local {
    pub id: LocalId,
    pub name: Option<String>,
    pub ty: IrType,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub insts: Vec<Inst>,
}

#[derive(Debug, Clone)]
pub enum Callee {
    Static(String),
    Local(LocalId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
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
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

#[derive(Debug, Clone)]
pub enum Inst {
    LoadConst {
        dest: LocalId,
        value: Const,
    },
    LoadFunction {
        dest: LocalId,
        name: String,
    },
    Move {
        dest: LocalId,
        src: LocalId,
    },
    Binary {
        dest: LocalId,
        op: BinOp,
        lhs: LocalId,
        rhs: LocalId,
    },
    Unary {
        dest: LocalId,
        op: UnOp,
        src: LocalId,
    },
    Call {
        dest: Option<LocalId>,
        callee: Callee,
        args: Vec<LocalId>,
    },
    Spawn {
        dest: LocalId,
        callee: Callee,
        args: Vec<LocalId>,
    },
    Await {
        dest: LocalId,
        task: LocalId,
    },
    GetIndex {
        dest: LocalId,
        obj: LocalId,
        index: LocalId,
    },
    SetIndex {
        obj: LocalId,
        index: LocalId,
        value: LocalId,
    },
    GetField {
        dest: LocalId,
        obj: LocalId,
        field: String,
    },
    SetField {
        obj: LocalId,
        field: String,
        value: LocalId,
    },
    MakeList {
        dest: LocalId,
        items: Vec<LocalId>,
    },
    MakeMap {
        dest: LocalId,
        keys: Vec<LocalId>,
        values: Vec<LocalId>,
    },
    MakeStruct {
        dest: LocalId,
        name: String,
        fields: Vec<(String, LocalId)>,
    },
    MakeRange {
        dest: LocalId,
        start: LocalId,
        end: LocalId,
    },
    MakeIter {
        dest: LocalId,
        src: LocalId,
    },
    IterNext {
        value: LocalId,
        more: LocalId,
        iter: LocalId,
    },
    Concat {
        dest: LocalId,
        parts: Vec<LocalId>,
    },
    Jump {
        target: BlockId,
    },
    Branch {
        cond: LocalId,
        then_block: BlockId,
        else_block: BlockId,
    },
    Return {
        value: Option<LocalId>,
    },
}

impl Inst {
    #[must_use]
    pub fn is_terminator(&self) -> bool {
        matches!(
            self,
            Self::Jump { .. } | Self::Branch { .. } | Self::Return { .. }
        )
    }
}

impl BinOp {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::Rem => "rem",
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Gt => "gt",
            Self::Ge => "ge",
            Self::And => "and",
            Self::Or => "or",
        }
    }
}

impl UnOp {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Neg => "neg",
            Self::Not => "not",
        }
    }
}

impl Function {
    pub fn block(&self, id: BlockId) -> Option<&BasicBlock> {
        self.blocks.iter().find(|b| b.id == id)
    }

    pub fn local(&self, id: LocalId) -> Option<&Local> {
        self.locals.iter().find(|l| l.id == id)
    }
}
