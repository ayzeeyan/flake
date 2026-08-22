//! IR types — a flattened view of Flake surface types.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrType {
    Nil,
    Bool,
    Int,
    Float,
    String,
    List(Box<IrType>),
    Map(Box<IrType>, Box<IrType>),
    Struct(String),
    Task(Box<IrType>),
    Range,
    Iter,
    Func(Box<IrType>),
    Dyn,
    Unknown,
}

impl fmt::Display for IrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => f.write_str("Nil"),
            Self::Bool => f.write_str("Bool"),
            Self::Int => f.write_str("Int"),
            Self::Float => f.write_str("Float"),
            Self::String => f.write_str("String"),
            Self::List(e) => write!(f, "[{e}]"),
            Self::Map(k, v) => write!(f, "Map[{k}, {v}]"),
            Self::Struct(n) => f.write_str(n),
            Self::Task(r) => write!(f, "Task[{r}]"),
            Self::Range => f.write_str("Range"),
            Self::Iter => f.write_str("Iter"),
            Self::Func(ret) => write!(f, "fn -> {ret}"),
            Self::Dyn => f.write_str("dyn"),
            Self::Unknown => f.write_str("?"),
        }
    }
}
