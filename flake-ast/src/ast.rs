//! Flake abstract syntax tree.

use crate::span::Span;

/// A complete Flake compilation unit.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
    pub span: Span,
}

/// A top-level declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Fn(FnDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    Type(TypeAlias),
    Import(ImportDecl),
}

impl Item {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Fn(f) => f.span,
            Self::Struct(s) => s.span,
            Self::Enum(e) => e.span,
            Self::Type(t) => t.span,
            Self::Import(i) => i.span,
        }
    }

    /// Whether this item was declared `pub`.
    #[must_use]
    pub fn is_pub(&self) -> bool {
        match self {
            Self::Fn(f) => f.is_pub,
            Self::Struct(s) => s.is_pub,
            Self::Enum(e) => e.is_pub,
            Self::Type(t) => t.is_pub,
            Self::Import(i) => i.is_pub,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

impl Ident {
    #[must_use]
    pub fn new(name: impl Into<String>, span: Span) -> Self {
        Self {
            name: name.into(),
            span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub is_pub: bool,
    /// Function is a `strict` ownership context.
    pub strict: bool,
    /// Function is an `owned` ownership context.
    pub owned: bool,
    pub name: Ident,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub effects: EffectSet,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: Ident,
    pub ty: Option<TypeExpr>,
    pub span: Span,
}

/// Effect clause on a function (`/ io + alloc`).
///
/// If `specified` is false, effects were omitted and should be inferred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSet {
    pub effects: Vec<Ident>,
    pub specified: bool,
    pub span: Span,
}

impl EffectSet {
    #[must_use]
    pub fn unspecified() -> Self {
        Self {
            effects: Vec::new(),
            specified: false,
            span: Span::DUMMY,
        }
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.effects.iter().map(|e| e.name.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub is_pub: bool,
    pub name: Ident,
    pub fields: Vec<StructField>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: Ident,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeAlias {
    pub is_pub: bool,
    pub name: Ident,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDecl {
    pub is_pub: bool,
    pub path: Ident,
    pub alias: Option<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub is_pub: bool,
    pub name: Ident,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: Ident,
    pub fields: Vec<TypeExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

impl Block {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stmts.is_empty() && self.tail.is_none()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let(LetStmt),
    Var(LetStmt),
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    While {
        cond: Expr,
        body: Block,
        span: Span,
    },
    For {
        name: Ident,
        iter: Expr,
        body: Block,
        span: Span,
    },
    Loop {
        body: Block,
        span: Span,
    },
    Expr(Expr),
}

impl Stmt {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Let(s) | Self::Var(s) => s.span,
            Self::Return { span, .. }
            | Self::Break { span }
            | Self::Continue { span }
            | Self::While { span, .. }
            | Self::For { span, .. }
            | Self::Loop { span, .. } => *span,
            Self::Expr(e) => e.span(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LetStmt {
    pub name: Ident,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal {
        value: Literal,
        span: Span,
    },
    Ident(Ident),
    Interpolated {
        parts: Vec<InterpPart>,
        span: Span,
    },
    List {
        elements: Vec<Expr>,
        span: Span,
    },
    Map {
        entries: Vec<(Expr, Expr)>,
        span: Span,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    Assign {
        op: AssignOp,
        target: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    /// Start a scope-bound task. The operand is restricted to a call expression.
    Spawn {
        call: Box<Expr>,
        span: Span,
    },
    /// Join a task and yield its result.
    Await {
        task: Box<Expr>,
        span: Span,
    },
    /// Unwrap `Result.Ok(value)` or return `Result.Err(error)` from the
    /// enclosing function.
    Try {
        expr: Box<Expr>,
        span: Span,
    },
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Field {
        target: Box<Expr>,
        field: Ident,
        span: Span,
    },
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        span: Span,
    },
    If {
        cond: Box<Expr>,
        then_block: Block,
        else_block: Option<Box<Expr>>,
        span: Span,
    },
    Block(Block),
    /// A scoped nursery block. All tasks spawned inside are scope-bound to the nursery.
    Nursery {
        body: Block,
        span: Span,
    },
    StructInit {
        name: Ident,
        fields: Vec<(Ident, Expr)>,
        span: Span,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wildcard {
        span: Span,
    },
    Literal {
        value: Literal,
        span: Span,
    },
    Ident(Ident),
    Variant {
        ty: Option<Ident>,
        variant: Ident,
        fields: Vec<Pattern>,
        span: Span,
    },
    List {
        patterns: Vec<Pattern>,
        span: Span,
    },
}

impl Pattern {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Wildcard { span }
            | Self::Literal { span, .. }
            | Self::Variant { span, .. }
            | Self::List { span, .. } => *span,
            Self::Ident(id) => id.span,
        }
    }
}

impl Expr {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Literal { span, .. }
            | Self::Interpolated { span, .. }
            | Self::List { span, .. }
            | Self::Map { span, .. }
            | Self::Unary { span, .. }
            | Self::Binary { span, .. }
            | Self::Assign { span, .. }
            | Self::Call { span, .. }
            | Self::Spawn { span, .. }
            | Self::Await { span, .. }
            | Self::Try { span, .. }
            | Self::Index { span, .. }
            | Self::Field { span, .. }
            | Self::Range { span, .. }
            | Self::If { span, .. }
            | Self::Nursery { span, .. }
            | Self::StructInit { span, .. }
            | Self::Match { span, .. } => *span,
            Self::Ident(id) => id.span,
            Self::Block(b) => b.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterpPart {
    Text(String),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
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

impl BinOp {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Rem => "%",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::And => "&&",
            Self::Or => "||",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    Ref,
    RefMut,
}

impl UnOp {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Neg => "-",
            Self::Not => "!",
            Self::Ref => "&",
            Self::RefMut => "&mut ",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    RemAssign,
}

impl AssignOp {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Assign => "=",
            Self::AddAssign => "+=",
            Self::SubAssign => "-=",
            Self::MulAssign => "*=",
            Self::DivAssign => "/=",
            Self::RemAssign => "%=",
        }
    }
}

/// Surface type expression, including optional ownership wrappers.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Dyn {
        span: Span,
    },
    Named {
        name: Ident,
        args: Vec<TypeExpr>,
        span: Span,
    },
    List {
        element: Box<TypeExpr>,
        span: Span,
    },
    Optional {
        inner: Box<TypeExpr>,
        span: Span,
    },
    Owned {
        inner: Box<TypeExpr>,
        span: Span,
    },
    Ref {
        mutable: bool,
        inner: Box<TypeExpr>,
        span: Span,
    },
    Mut {
        inner: Box<TypeExpr>,
        span: Span,
    },
    Fn {
        params: Vec<TypeExpr>,
        ret: Option<Box<TypeExpr>>,
        effects: EffectSet,
        span: Span,
    },
}

impl TypeExpr {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Dyn { span }
            | Self::Named { span, .. }
            | Self::List { span, .. }
            | Self::Optional { span, .. }
            | Self::Owned { span, .. }
            | Self::Ref { span, .. }
            | Self::Mut { span, .. }
            | Self::Fn { span, .. } => *span,
        }
    }
}
