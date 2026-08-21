//! Runtime values for the bytecode VM.

use std::cell::RefCell;
use std::collections::HashMap;
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
    Map(Rc<RefCell<HashMap<String, Value>>>),
    Struct {
        name: Rc<str>,
        fields: Rc<RefCell<HashMap<String, Value>>>,
    },
    Function(Rc<Function>),
    Native(Native),
    Range {
        start: i64,
        end: i64,
    },
    Iter(Rc<RefCell<Iter>>),
}

#[derive(Clone)]
pub struct Iter {
    pub kind: IterKind,
}

#[derive(Clone)]
pub enum IterKind {
    List {
        items: Rc<RefCell<Vec<Value>>>,
        idx: usize,
    },
    Range {
        next: i64,
        end: i64,
        rev: bool,
    },
    Chars {
        chars: Vec<char>,
        idx: usize,
    },
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
    Len,
    Push,
    Pop,
    Str,
    Int,
    Float,
    TypeOf,
    Assert,
    ReadFile,
    Abs,
    Min,
    Max,
    Range,
    Join,
    Split,
}

impl Native {
    pub fn all() -> [Native; 16] {
        [
            Native::Print,
            Native::Len,
            Native::Push,
            Native::Pop,
            Native::Str,
            Native::Int,
            Native::Float,
            Native::TypeOf,
            Native::Assert,
            Native::ReadFile,
            Native::Abs,
            Native::Min,
            Native::Max,
            Native::Range,
            Native::Join,
            Native::Split,
        ]
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Print => "print",
            Self::Len => "len",
            Self::Push => "push",
            Self::Pop => "pop",
            Self::Str => "str",
            Self::Int => "int",
            Self::Float => "float",
            Self::TypeOf => "type_of",
            Self::Assert => "assert",
            Self::ReadFile => "read_file",
            Self::Abs => "abs",
            Self::Min => "min",
            Self::Max => "max",
            Self::Range => "range",
            Self::Join => "join",
            Self::Split => "split",
        }
    }
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
            Self::Map(_) => "Map",
            Self::Struct { .. } => "Struct",
            Self::Function(_) | Self::Native(_) => "Function",
            Self::Range { .. } => "Range",
            Self::Iter(_) => "Iter",
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
            Self::Map(map) => {
                let map = map.borrow();
                let inner: Vec<_> = map
                    .iter()
                    .map(|(k, v)| format!("{k:?}: {}", v.repr()))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
            Self::Struct { name, fields } => {
                let fields = fields.borrow();
                let inner: Vec<_> = fields
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", v.repr()))
                    .collect();
                format!("{name} {{ {} }}", inner.join(", "))
            }
            Self::Function(f) => format!("<fn {}>", f.name),
            Self::Native(n) => format!("<fn {}>", n.name()),
            Self::Range { start, end } => format!("{start}..{end}"),
            Self::Iter(_) => "<iter>".into(),
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
            (Self::Range { start: a, end: b }, Self::Range { start: c, end: d }) => a == c && b == d,
            (Self::Native(a), Self::Native(b)) => a == b,
            (Self::Function(a), Self::Function(b)) => Rc::ptr_eq(a, b),
            (Self::List(a), Self::List(b)) => {
                if Rc::ptr_eq(a, b) {
                    return true;
                }
                let a = a.borrow();
                let b = b.borrow();
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y))
            }
            (
                Self::Struct {
                    name: n1,
                    fields: f1,
                },
                Self::Struct {
                    name: n2,
                    fields: f2,
                },
            ) => {
                n1 == n2 && {
                    let f1 = f1.borrow();
                    let f2 = f2.borrow();
                    f1.len() == f2.len()
                        && f1.iter().all(|(k, v)| f2.get(k).is_some_and(|o| v.equals(o)))
                }
            }
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

impl Iter {
    pub fn next_value(&mut self) -> Option<Value> {
        match &mut self.kind {
            IterKind::List { items, idx } => {
                let items = items.borrow();
                if *idx >= items.len() {
                    return None;
                }
                let v = items[*idx].clone();
                *idx += 1;
                Some(v)
            }
            IterKind::Range { next, end, rev } => {
                if *rev {
                    if *next <= *end {
                        return None;
                    }
                    let v = Value::Int(*next);
                    *next -= 1;
                    Some(v)
                } else {
                    if *next >= *end {
                        return None;
                    }
                    let v = Value::Int(*next);
                    *next += 1;
                    Some(v)
                }
            }
            IterKind::Chars { chars, idx } => {
                if *idx >= chars.len() {
                    return None;
                }
                let ch = chars[*idx];
                *idx += 1;
                Some(Value::from_string(ch.to_string()))
            }
        }
    }
}
