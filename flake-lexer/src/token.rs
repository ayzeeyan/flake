//! Token kinds produced by the Flake lexer.

use std::fmt;

use flake_ast::Span;

/// A single token with its source span.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    #[must_use]
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    #[must_use]
    pub fn is_eof(&self) -> bool {
        self.kind == TokenKind::Eof
    }
}

/// Lexical token kinds.
///
/// Identifier spelling is recovered from the source span. String contents are
/// stored decoded on `StringText` because escapes have already been processed.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident,
    Int(i64),
    Float(f64),

    /// Opening `"` of a string (interpolated or not).
    StringStart,
    /// Decoded run of characters inside a string.
    StringText(String),
    /// `{` that opens interpolation inside a string.
    InterpOpen,
    /// `}` that closes interpolation and returns to string mode.
    InterpClose,
    /// Closing `"` of a string.
    StringEnd,

    // Keywords
    Fn,
    Let,
    Var,
    If,
    Else,
    While,
    For,
    Loop,
    In,
    Return,
    Break,
    Continue,
    True,
    False,
    Nil,
    Dyn,
    Type,
    Struct,
    Enum,
    Strict,
    Owned,
    Ref,
    Mut,
    Import,
    As,
    Pub,
    Unsafe,
    Match,
    Spawn,
    Await,
    Nursery,
    Trait,
    Impl,
    Const,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    BangEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    AmpAmp,
    PipePipe,
    Bang,
    Eq,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    Arrow,
    FatArrow,
    Amp,
    Dot,
    DotDot,
    Question,

    // Punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semicolon,

    /// One or more consecutive newlines in source mode (statement separator).
    Newline,
    Eof,
}

impl TokenKind {
    #[must_use]
    pub fn keyword(ident: &str) -> Option<Self> {
        Some(match ident {
            "fn" => Self::Fn,
            "let" => Self::Let,
            "var" => Self::Var,
            "if" => Self::If,
            "else" => Self::Else,
            "while" => Self::While,
            "for" => Self::For,
            "loop" => Self::Loop,
            "in" => Self::In,
            "return" => Self::Return,
            "break" => Self::Break,
            "continue" => Self::Continue,
            "true" => Self::True,
            "false" => Self::False,
            "nil" => Self::Nil,
            "dyn" => Self::Dyn,
            "type" => Self::Type,
            "struct" => Self::Struct,
            "enum" => Self::Enum,
            "strict" => Self::Strict,
            "owned" => Self::Owned,
            "ref" => Self::Ref,
            "mut" => Self::Mut,
            "import" => Self::Import,
            "as" => Self::As,
            "pub" => Self::Pub,
            "unsafe" => Self::Unsafe,
            "match" => Self::Match,
            "spawn" => Self::Spawn,
            "await" => Self::Await,
            "nursery" => Self::Nursery,
            "trait" => Self::Trait,
            "impl" => Self::Impl,
            "const" => Self::Const,
            _ => return None,
        })
    }

    #[must_use]
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            Self::Fn
                | Self::Let
                | Self::Var
                | Self::If
                | Self::Else
                | Self::While
                | Self::For
                | Self::Loop
                | Self::In
                | Self::Return
                | Self::Break
                | Self::Continue
                | Self::True
                | Self::False
                | Self::Nil
                | Self::Dyn
                | Self::Type
                | Self::Struct
                | Self::Enum
                | Self::Strict
                | Self::Owned
                | Self::Ref
                | Self::Mut
                | Self::Import
                | Self::As
                | Self::Pub
                | Self::Unsafe
                | Self::Match
                | Self::Spawn
                | Self::Await
                | Self::Nursery
                | Self::Trait
                | Self::Impl
                | Self::Const
        )
    }

    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ident => "identifier",
            Self::Int(_) => "integer",
            Self::Float(_) => "float",
            Self::StringStart => "`\"`",
            Self::StringText(_) => "string text",
            Self::InterpOpen => "`{` (interpolation)",
            Self::InterpClose => "`}` (interpolation)",
            Self::StringEnd => "`\"`",
            Self::Fn => "`fn`",
            Self::Let => "`let`",
            Self::Var => "`var`",
            Self::If => "`if`",
            Self::Else => "`else`",
            Self::While => "`while`",
            Self::For => "`for`",
            Self::Loop => "`loop`",
            Self::In => "`in`",
            Self::Return => "`return`",
            Self::Break => "`break`",
            Self::Continue => "`continue`",
            Self::True => "`true`",
            Self::False => "`false`",
            Self::Nil => "`nil`",
            Self::Dyn => "`dyn`",
            Self::Type => "`type`",
            Self::Struct => "`struct`",
            Self::Enum => "`enum`",
            Self::Strict => "`strict`",
            Self::Owned => "`owned`",
            Self::Ref => "`ref`",
            Self::Mut => "`mut`",
            Self::Import => "`import`",
            Self::As => "`as`",
            Self::Pub => "`pub`",
            Self::Unsafe => "`unsafe`",
            Self::Match => "`match`",
            Self::Spawn => "`spawn`",
            Self::Await => "`await`",
            Self::Nursery => "`nursery`",
            Self::Trait => "`trait`",
            Self::Impl => "`impl`",
            Self::Const => "`const`",
            Self::Plus => "`+`",
            Self::Minus => "`-`",
            Self::Star => "`*`",
            Self::Slash => "`/`",
            Self::Percent => "`%`",
            Self::EqEq => "`==`",
            Self::BangEq => "`!=`",
            Self::Lt => "`<`",
            Self::Gt => "`>`",
            Self::LtEq => "`<=`",
            Self::GtEq => "`>=`",
            Self::AmpAmp => "`&&`",
            Self::PipePipe => "`||`",
            Self::Bang => "`!`",
            Self::Eq => "`=`",
            Self::PlusEq => "`+=`",
            Self::MinusEq => "`-=`",
            Self::StarEq => "`*=`",
            Self::SlashEq => "`/=`",
            Self::PercentEq => "`%=`",
            Self::Arrow => "`->`",
            Self::FatArrow => "`=>`",
            Self::Amp => "`&`",
            Self::Dot => "`.`",
            Self::DotDot => "`..`",
            Self::Question => "`?`",
            Self::LParen => "`(`",
            Self::RParen => "`)`",
            Self::LBrace => "`{`",
            Self::RBrace => "`}`",
            Self::LBracket => "`[`",
            Self::RBracket => "`]`",
            Self::Comma => "`,`",
            Self::Colon => "`:`",
            Self::Semicolon => "`;`",
            Self::Newline => "newline",
            Self::Eof => "end of file",
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(n) => write!(f, "{n}"),
            Self::Float(n) => write!(f, "{n}"),
            Self::StringText(s) => write!(f, "{s:?}"),
            other => f.write_str(other.as_str()),
        }
    }
}
