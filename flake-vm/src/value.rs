//! Runtime values for the bytecode VM.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use crate::opcode::Chunk;

#[derive(Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Rc<str>),
    List(Rc<RefCell<Vec<Value>>>),
    Function(Rc<Function>),
    Native(Native),
}

#[derive(Clone)]
pub struct Function {
    pub name: String,
    pub arity: u8,
    pub chunk: Chunk,
    pub locals: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Native {
    Print,
}

impl Value {
    pub fn from_string(s: impl Into<String>) -> Self {
        Self::String(Rc::from(s.into()))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Nil => "Nil",
            Self::Bool(_) => "Bool",
            Self::Int(_) => "Int",
            Self::Float(_) => "Float",
            Self::String(_) => "String",
            Self::List(_) => "List",
            Self::Function(_) | Self::Native(_) => "Function",
        }
    }

    pub fn display_value(&self) -> String {
        match self {
            Self::Nil => "nil".into(),
            Self::Bool(b) => b.to_string(),
            Self::Int(n) => n.to_string(),
            Self::Float(n) => n.to_string(),
            Self::String(s) => s.to_string(),
            Self::List(xs) => {
                let xs = xs.borrow();
                let inner: Vec<_> = xs.iter().map(Self::repr).collect();
                format!("[{}]", inner.join(", "))
            }
            Self::Function(f) => format!("<fn {}>", f.name),
            Self::Native(Native::Print) => "<fn print>".into(),
        }
    }

    pub fn repr(&self) -> String {
        match self {
            Self::String(s) => format!("{s:?}"),
            other => other.display_value(),
        }
    }

    pub fn equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Self::Nil, Self::Nil) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a == b,
            (Self::Int(a), Self::Float(b)) => *a as f64 == *b,
            (Self::Float(a), Self::Int(b)) => *a == *b as f64,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Native(a), Self::Native(b)) => a == b,
            (Self::Function(a), Self::Function(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    pub fn as_bool(&self) -> Result<bool, String> {
        match self {
            Self::Bool(b) => Ok(*b),
            other => Err(format!("expected Bool, found {}", other.type_name())),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.repr())
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display_value())
    }
}

impl fmt::Debug for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<fn {}>", self.name)
    }
}
