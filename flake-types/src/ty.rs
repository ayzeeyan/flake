//! Type representation.

use std::fmt;

use crate::effects::EffectSet;

/// A Flake type, including gradual `dyn` and inference variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Nil,
    Bool,
    Int,
    Float,
    String,
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    /// Scope-bound result of a `spawn` expression.
    Task(Box<Type>),
    Struct {
        name: String,
        fields: Vec<(String, Type)>,
    },
    Fn {
        params: Vec<Type>,
        ret: Box<Type>,
        effects: EffectSet,
    },
    Range,
    Optional(Box<Type>),
    Dyn,
    Var(u32),
    Owned(Box<Type>),
    Ref {
        mutable: bool,
        inner: Box<Type>,
    },
    Mut(Box<Type>),
    /// Namespace introduced by `import math` / `import math as m`.
    Module {
        name: String,
        members: Vec<(String, Type)>,
    },
    Enum {
        name: String,
        variants: Vec<(String, Vec<Type>)>,
    },
}

impl Type {
    #[must_use]
    pub fn list(elem: Type) -> Self {
        Self::List(Box::new(elem))
    }

    #[must_use]
    pub fn function(params: Vec<Type>, ret: Type, effects: EffectSet) -> Self {
        Self::Fn {
            params,
            ret: Box::new(ret),
            effects,
        }
    }

    /// Strip ownership wrappers for ordinary (non-strict) type comparison.
    #[must_use]
    pub fn without_ownership(&self) -> Type {
        match self {
            Self::Owned(inner) | Self::Mut(inner) => inner.without_ownership(),
            Self::Ref { inner, .. } => inner.without_ownership(),
            Self::List(e) => Self::list(e.without_ownership()),
            Self::Map(k, v) => Self::Map(
                Box::new(k.without_ownership()),
                Box::new(v.without_ownership()),
            ),
            Self::Task(result) => Self::Task(Box::new(result.without_ownership())),
            Self::Optional(i) => Self::Optional(Box::new(i.without_ownership())),
            Self::Fn {
                params,
                ret,
                effects,
            } => Self::Fn {
                params: params.iter().map(Self::without_ownership).collect(),
                ret: Box::new(ret.without_ownership()),
                effects: effects.clone(),
            },
            Self::Struct { name, fields } => Self::Struct {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(|(n, t)| (n.clone(), t.without_ownership()))
                    .collect(),
            },
            Self::Module { name, members } => Self::Module {
                name: name.clone(),
                members: members
                    .iter()
                    .map(|(n, t)| (n.clone(), t.without_ownership()))
                    .collect(),
            },
            Self::Enum { name, variants } => Self::Enum {
                name: name.clone(),
                variants: variants
                    .iter()
                    .map(|(n, ts)| (n.clone(), ts.iter().map(Self::without_ownership).collect()))
                    .collect(),
            },
            other => other.clone(),
        }
    }

    #[must_use]
    pub fn is_dyn(&self) -> bool {
        matches!(self, Self::Dyn)
    }

    #[must_use]
    pub fn contains_ref(&self) -> bool {
        match self {
            Self::Ref { .. } => true,
            Self::Owned(inner) | Self::Mut(inner) | Self::Optional(inner) | Self::Task(inner) => {
                inner.contains_ref()
            }
            Self::List(elem) => elem.contains_ref(),
            Self::Map(k, v) => k.contains_ref() || v.contains_ref(),
            Self::Struct { fields, .. } => fields.iter().any(|(_, t)| t.contains_ref()),
            Self::Enum { variants, .. } => {
                variants.iter().any(|(_, ts)| ts.iter().any(|t| t.contains_ref()))
            }
            Self::Fn { params, ret, .. } => {
                params.iter().any(|t| t.contains_ref()) || ret.contains_ref()
            }
            _ => false,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        format!("{self}")
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => f.write_str("Nil"),
            Self::Bool => f.write_str("Bool"),
            Self::Int => f.write_str("Int"),
            Self::Float => f.write_str("Float"),
            Self::String => f.write_str("String"),
            Self::List(e) => write!(f, "[{e}]"),
            Self::Map(k, v) => write!(f, "Map[{k}, {v}]"),
            Self::Task(result) => write!(f, "Task[{result}]"),
            Self::Struct { name, .. } => f.write_str(name),
            Self::Module { name, .. } => write!(f, "module {name}"),
            Self::Enum { name, .. } => f.write_str(name),
            Self::Fn {
                params,
                ret,
                effects,
            } => {
                write!(f, "fn(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{p}")?;
                }
                write!(f, ") -> {ret}")?;
                if effects.specified {
                    write!(f, " / {effects}")?;
                }
                Ok(())
            }
            Self::Range => f.write_str("Range"),
            Self::Optional(i) => write!(f, "{i}?"),
            Self::Dyn => f.write_str("dyn"),
            Self::Var(id) => write!(f, "?{id}"),
            Self::Owned(i) => write!(f, "owned {i}"),
            Self::Ref {
                mutable: true,
                inner,
            } => write!(f, "&mut {inner}"),
            Self::Ref {
                mutable: false,
                inner,
            } => write!(f, "&{inner}"),
            Self::Mut(i) => write!(f, "mut {i}"),
        }
    }
}
