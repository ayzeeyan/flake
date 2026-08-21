//! Runtime values.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use flake_ast::{Block, Ident, Span};

use crate::env::Env;

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
    Native(NativeFn),
    Range {
        start: i64,
        end: i64,
    },
    Module {
        name: Rc<str>,
        members: Rc<HashMap<String, Value>>,
    },
    Enum {
        type_name: Rc<str>,
        variant: Rc<str>,
        tag: i64,
        fields: Vec<Value>,
    },
    VariantCtor {
        type_name: Rc<str>,
        variant: Rc<str>,
        tag: i64,
        arity: usize,
    },
}

#[derive(Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<Ident>,
    pub body: Block,
    pub closure: Env,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeFn {
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
    WriteFile,
    Contains,
    StartsWith,
    EndsWith,
    First,
    Last,
}

impl NativeFn {
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
            Self::WriteFile => "write_file",
            Self::Contains => "contains",
            Self::StartsWith => "starts_with",
            Self::EndsWith => "ends_with",
            Self::First => "first",
            Self::Last => "last",
        }
    }
}

impl Value {
    #[must_use]
    pub fn nil() -> Self {
        Self::Nil
    }

    #[must_use]
    pub fn from_string(s: impl Into<String>) -> Self {
        Self::String(Rc::from(s.into()))
    }

    #[must_use]
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
            Self::Function(_) => "Function",
            Self::Native(_) => "Function",
            Self::Range { .. } => "Range",
            Self::Module { .. } => "Module",
            Self::Enum { .. } => "Enum",
            Self::VariantCtor { .. } => "Function",
        }
    }

    #[must_use]
    pub fn is_nil(&self) -> bool {
        matches!(self, Self::Nil)
    }

    pub fn as_bool(&self, span: Span) -> Result<bool, crate::error::RuntimeError> {
        match self {
            Self::Bool(b) => Ok(*b),
            other => Err(crate::error::RuntimeError::new(
                span,
                format!("expected Bool, found {}", other.type_name()),
            )),
        }
    }

    /// User-facing display used by `print` and string interpolation.
    #[must_use]
    pub fn display_value(&self) -> String {
        match self {
            Self::Nil => "nil".into(),
            Self::Bool(b) => b.to_string(),
            Self::Int(n) => n.to_string(),
            Self::Float(n) => n.to_string(),
            Self::String(s) => s.to_string(),
            Self::List(items) => {
                let items = items.borrow();
                let inner: Vec<_> = items.iter().map(Self::repr).collect();
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
            Self::Module { name, .. } => format!("<module {name}>"),
            Self::Enum {
                type_name,
                variant,
                fields,
                ..
            } => {
                if fields.is_empty() {
                    format!("{type_name}.{variant}")
                } else {
                    let inner: Vec<_> = fields.iter().map(Self::display_value).collect();
                    format!("{type_name}.{variant}({})", inner.join(", "))
                }
            }
            Self::VariantCtor {
                type_name, variant, ..
            } => format!("<ctor {type_name}.{variant}>"),
        }
    }

    #[must_use]
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
            (Self::Range { start: a, end: b }, Self::Range { start: c, end: d }) => {
                a == c && b == d
            }
            (Self::List(a), Self::List(b)) => {
                if Rc::ptr_eq(a, b) {
                    return true;
                }
                let a = a.borrow();
                let b = b.borrow();
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y))
            }
            (Self::Function(a), Self::Function(b)) => Rc::ptr_eq(a, b),
            (Self::Native(a), Self::Native(b)) => a == b,
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
                        && f1
                            .iter()
                            .all(|(k, v)| f2.get(k).is_some_and(|o| v.equals(o)))
                }
            }
            (
                Self::Enum {
                    type_name: t1,
                    variant: v1,
                    fields: f1,
                    ..
                },
                Self::Enum {
                    type_name: t2,
                    variant: v2,
                    fields: f2,
                    ..
                },
            ) => {
                t1 == t2
                    && v1 == v2
                    && f1.len() == f2.len()
                    && f1.iter().zip(f2).all(|(a, b)| a.equals(b))
            }
            _ => false,
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
