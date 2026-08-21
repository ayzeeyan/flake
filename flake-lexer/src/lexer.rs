//! Hand-written Flake lexer.

use flake_ast::{Source, Span};

use crate::error::LexError;
use crate::token::{Token, TokenKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Ordinary source (not inside a string interpolation).
    Source,
    /// Inside a `"..."` string.
    String,
    /// Expression nested in `{...}` inside a string.
    Interp { brace_depth: u32 },
}

/// Tokenize `source` into a complete token stream ending with [`TokenKind::Eof`].
pub fn tokenize(source: &Source) -> Result<Vec<Token>, LexError> {
    Lexer::new(source.text()).tokenize()
}

/// Tokenize a raw string. The synthetic file name is `<input>`.
pub fn tokenize_str(text: &str) -> Result<Vec<Token>, LexError> {
    tokenize(&Source::new("<input>", text))
}

struct Lexer<'src> {
    src: &'src str,
    pos: usize,
    tokens: Vec<Token>,
    modes: Vec<Mode>,
}

impl<'src> Lexer<'src> {
    fn new(src: &'src str) -> Self {
        Self {
            src,
            pos: 0,
            tokens: Vec::new(),
            modes: vec![Mode::Source],
        }
    }

    fn tokenize(mut self) -> Result<Vec<Token>, LexError> {
        while !self.is_eof() {
            match self.mode() {
                Mode::Source | Mode::Interp { .. } => self.lex_source()?,
                Mode::String => self.lex_string()?,
            }
        }

        match self.mode() {
            Mode::String => {
                return Err(LexError::new(
                    Span::point(self.pos as u32),
                    "unterminated string literal",
                ));
            }
            Mode::Interp { .. } => {
                return Err(LexError::new(
                    Span::point(self.pos as u32),
                    "unterminated string interpolation",
                ));
            }
            Mode::Source => {}
        }

        self.push(TokenKind::Eof, self.pos, self.pos);
        Ok(self.tokens)
    }

    fn mode(&self) -> Mode {
        *self.modes.last().expect("mode stack is never empty")
    }

    fn lex_source(&mut self) -> Result<(), LexError> {
        let trivia_newline = self.skip_space_and_comments()?;
        if trivia_newline && matches!(self.mode(), Mode::Source) {
            let at_real_newline = matches!(self.peek(), Some('\n' | '\r'));
            if !at_real_newline && !self.is_eof() {
                let pos = self.pos;
                self.push(TokenKind::Newline, pos, pos);
            }
        }
        if self.is_eof() {
            return Ok(());
        }
        if matches!(self.mode(), Mode::String) {
            return Ok(());
        }

        let start = self.pos;
        let c = self.bump().expect("not eof");

        match c {
            '\n' => self.lex_newlines(start),
            '\r' => {
                self.eat('\n');
                self.lex_newlines(start);
            }
            '"' => {
                self.push(TokenKind::StringStart, start, self.pos);
                self.modes.push(Mode::String);
            }
            '(' => self.push(TokenKind::LParen, start, self.pos),
            ')' => self.push(TokenKind::RParen, start, self.pos),
            '{' => {
                if let Mode::Interp { brace_depth } = self.modes.last_mut().unwrap() {
                    *brace_depth += 1;
                }
                self.push(TokenKind::LBrace, start, self.pos);
            }
            '}' => match self.modes.last().copied() {
                Some(Mode::Interp { brace_depth: 0 }) => {
                    self.push(TokenKind::InterpClose, start, self.pos);
                    self.modes.pop();
                }
                Some(Mode::Interp { .. }) => {
                    if let Mode::Interp { brace_depth } = self.modes.last_mut().unwrap() {
                        *brace_depth -= 1;
                    }
                    self.push(TokenKind::RBrace, start, self.pos);
                }
                _ => self.push(TokenKind::RBrace, start, self.pos),
            },
            '[' => self.push(TokenKind::LBracket, start, self.pos),
            ']' => self.push(TokenKind::RBracket, start, self.pos),
            ',' => self.push(TokenKind::Comma, start, self.pos),
            ':' => self.push(TokenKind::Colon, start, self.pos),
            ';' => self.push(TokenKind::Semicolon, start, self.pos),
            '?' => self.push(TokenKind::Question, start, self.pos),
            '+' => self.lex_compound(start, TokenKind::Plus, '=', TokenKind::PlusEq),
            '*' => self.lex_compound(start, TokenKind::Star, '=', TokenKind::StarEq),
            '%' => self.lex_compound(start, TokenKind::Percent, '=', TokenKind::PercentEq),
            '-' => {
                if self.eat('>') {
                    self.push(TokenKind::Arrow, start, self.pos);
                } else if self.eat('=') {
                    self.push(TokenKind::MinusEq, start, self.pos);
                } else {
                    self.push(TokenKind::Minus, start, self.pos);
                }
            }
            '/' => {
                // Comments are handled in skip; a leftover `/` is an operator.
                if self.eat('=') {
                    self.push(TokenKind::SlashEq, start, self.pos);
                } else {
                    self.push(TokenKind::Slash, start, self.pos);
                }
            }
            '=' => {
                if self.eat('>') {
                    self.push(TokenKind::FatArrow, start, self.pos);
                } else if self.eat('=') {
                    self.push(TokenKind::EqEq, start, self.pos);
                } else {
                    self.push(TokenKind::Eq, start, self.pos);
                }
            }
            '!' => self.lex_compound(start, TokenKind::Bang, '=', TokenKind::BangEq),
            '<' => self.lex_compound(start, TokenKind::Lt, '=', TokenKind::LtEq),
            '>' => self.lex_compound(start, TokenKind::Gt, '=', TokenKind::GtEq),
            '&' => self.lex_compound(start, TokenKind::Amp, '&', TokenKind::AmpAmp),
            '|' => {
                if self.eat('|') {
                    self.push(TokenKind::PipePipe, start, self.pos);
                } else {
                    return Err(LexError::new(
                        Span::new(start as u32, self.pos as u32),
                        "unexpected `|`; use `||` for logical or",
                    ));
                }
            }
            '.' => {
                if self.eat('.') {
                    self.push(TokenKind::DotDot, start, self.pos);
                } else if self.peek().is_some_and(|d| d.is_ascii_digit()) {
                    self.lex_number(start, true)?;
                } else {
                    self.push(TokenKind::Dot, start, self.pos);
                }
            }
            c if is_ident_start(c) => self.lex_ident(start),
            c if c.is_ascii_digit() => self.lex_number(start, false)?,
            c => {
                return Err(LexError::new(
                    Span::new(start as u32, self.pos as u32),
                    format!("unexpected character {c:?}"),
                ));
            }
        }
        Ok(())
    }

    fn lex_newlines(&mut self, start: usize) {
        loop {
            self.skip_horizontal_space();
            if self.eat('\n') {
                continue;
            }
            if self.peek() == Some('\r') {
                self.bump();
                self.eat('\n');
                continue;
            }
            break;
        }
        // Newlines inside interpolation expressions are insignificant.
        if matches!(self.mode(), Mode::Source) {
            self.push(TokenKind::Newline, start, self.pos);
        }
    }

    fn lex_ident(&mut self, start: usize) {
        while self.peek().is_some_and(is_ident_continue) {
            self.bump();
        }
        let text = &self.src[start..self.pos];
        let kind = TokenKind::keyword(text).unwrap_or(TokenKind::Ident);
        self.push(kind, start, self.pos);
    }

    fn lex_number(&mut self, start: usize, leading_dot: bool) -> Result<(), LexError> {
        if leading_dot {
            self.consume_digits_with_underscores(true)?;
            self.maybe_exponent()?;
            return self.finish_float(start);
        }

        if self.src.as_bytes().get(start) == Some(&b'0') {
            match self.peek() {
                Some('x' | 'X') => {
                    self.bump();
                    return self.lex_radix_int(start, 16, "hexadecimal");
                }
                Some('b' | 'B') => {
                    self.bump();
                    return self.lex_radix_int(start, 2, "binary");
                }
                Some('o' | 'O') => {
                    self.bump();
                    return self.lex_radix_int(start, 8, "octal");
                }
                _ => {}
            }
        }

        // The first digit was already consumed by the caller.
        self.consume_digits_with_underscores(false)?;

        // `1..10` is an integer followed by `..`, not a float.
        if self.peek() == Some('.') && self.peek2() != Some('.') {
            if self.peek2().is_some_and(|c| c.is_ascii_digit()) {
                self.bump(); // '.'
                self.consume_digits_with_underscores(true)?;
                self.maybe_exponent()?;
                return self.finish_float(start);
            }
        }

        if matches!(self.peek(), Some('e' | 'E')) {
            self.maybe_exponent()?;
            return self.finish_float(start);
        }

        let lexeme = strip_underscores(&self.src[start..self.pos]);
        match lexeme.parse::<i64>() {
            Ok(value) => {
                self.push(TokenKind::Int(value), start, self.pos);
                Ok(())
            }
            Err(_) => Err(LexError::new(
                Span::new(start as u32, self.pos as u32),
                "integer literal is out of range for a 64-bit signed integer",
            )),
        }
    }

    fn lex_radix_int(&mut self, start: usize, radix: u32, name: &str) -> Result<(), LexError> {
        let digits_start = self.pos;
        let mut saw_digit = false;
        loop {
            match self.peek() {
                Some('_') => {
                    self.bump();
                }
                Some(c) if c.is_digit(radix) => {
                    saw_digit = true;
                    self.bump();
                }
                _ => break,
            }
        }
        if !saw_digit {
            return Err(LexError::new(
                Span::new(start as u32, self.pos as u32),
                format!("expected {name} digit"),
            ));
        }
        let lexeme = strip_underscores(&self.src[digits_start..self.pos]);
        match i64::from_str_radix(&lexeme, radix) {
            Ok(value) => {
                self.push(TokenKind::Int(value), start, self.pos);
                Ok(())
            }
            Err(_) => Err(LexError::new(
                Span::new(start as u32, self.pos as u32),
                format!("{name} literal is out of range for a 64-bit signed integer"),
            )),
        }
    }

    fn consume_digits_with_underscores(&mut self, need_digit: bool) -> Result<(), LexError> {
        let start = self.pos;
        let mut last_underscore = false;
        let mut saw_digit = false;
        loop {
            match self.peek() {
                Some('_') => {
                    last_underscore = true;
                    self.bump();
                }
                Some(c) if c.is_ascii_digit() => {
                    last_underscore = false;
                    saw_digit = true;
                    self.bump();
                }
                _ => break,
            }
        }
        if need_digit && !saw_digit {
            return Err(LexError::new(
                Span::new(start as u32, self.pos as u32),
                "expected digit",
            ));
        }
        if last_underscore {
            return Err(LexError::new(
                Span::new(start as u32, self.pos as u32),
                "number literal cannot end with `_`",
            ));
        }
        Ok(())
    }

    fn maybe_exponent(&mut self) -> Result<(), LexError> {
        if !matches!(self.peek(), Some('e' | 'E')) {
            return Ok(());
        }
        self.bump();
        if matches!(self.peek(), Some('+' | '-')) {
            self.bump();
        }
        self.consume_digits_with_underscores(true)
    }

    fn finish_float(&mut self, start: usize) -> Result<(), LexError> {
        let lexeme = strip_underscores(&self.src[start..self.pos]);
        match lexeme.parse::<f64>() {
            Ok(value) if value.is_finite() => {
                self.push(TokenKind::Float(value), start, self.pos);
                Ok(())
            }
            _ => Err(LexError::new(
                Span::new(start as u32, self.pos as u32),
                "invalid floating-point literal",
            )),
        }
    }

    fn lex_string(&mut self) -> Result<(), LexError> {
        let start = self.pos;
        let mut buf = String::new();
        let mut buf_start = start;

        while !self.is_eof() {
            let c = self.peek().unwrap();
            match c {
                '"' => {
                    self.flush_string_text(buf_start, &buf);
                    let end_start = self.pos;
                    self.bump();
                    self.push(TokenKind::StringEnd, end_start, self.pos);
                    self.modes.pop();
                    return Ok(());
                }
                '{' => {
                    self.flush_string_text(buf_start, &buf);
                    let brace_start = self.pos;
                    self.bump();
                    self.push(TokenKind::InterpOpen, brace_start, self.pos);
                    self.modes.push(Mode::Interp { brace_depth: 0 });
                    return Ok(());
                }
                '\\' => {
                    if buf.is_empty() {
                        buf_start = self.pos;
                    }
                    self.bump();
                    let escaped = self.lex_escape()?;
                    buf.push(escaped);
                }
                '\n' | '\r' => {
                    return Err(LexError::new(
                        Span::new(start as u32, self.pos as u32),
                        "unterminated string literal (line break in string; use `\\n` instead)",
                    ));
                }
                _ => {
                    if buf.is_empty() {
                        buf_start = self.pos;
                    }
                    buf.push(c);
                    self.bump();
                }
            }
        }

        Err(LexError::new(
            Span::new(start as u32, self.pos as u32),
            "unterminated string literal",
        ))
    }

    fn flush_string_text(&mut self, start: usize, buf: &str) {
        if buf.is_empty() {
            return;
        }
        self.push(TokenKind::StringText(buf.to_string()), start, self.pos);
    }

    fn lex_escape(&mut self) -> Result<char, LexError> {
        let start = self.pos.saturating_sub(1);
        let Some(c) = self.bump() else {
            return Err(LexError::new(
                Span::new(start as u32, self.pos as u32),
                "unterminated escape sequence",
            ));
        };
        match c {
            'n' => Ok('\n'),
            't' => Ok('\t'),
            'r' => Ok('\r'),
            '0' => Ok('\0'),
            '\\' => Ok('\\'),
            '"' => Ok('"'),
            '\'' => Ok('\''),
            '{' => Ok('{'),
            '}' => Ok('}'),
            'u' => self.lex_unicode_escape(start),
            other => Err(LexError::new(
                Span::new(start as u32, self.pos as u32),
                format!("unknown string escape `\\{other}`"),
            )),
        }
    }

    fn lex_unicode_escape(&mut self, start: usize) -> Result<char, LexError> {
        if !self.eat('{') {
            return Err(LexError::new(
                Span::new(start as u32, self.pos as u32),
                "unicode escape must look like `\\u{1F31F}`",
            ));
        }
        let hex_start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
            self.bump();
        }
        if !self.eat('}') {
            return Err(LexError::new(
                Span::new(start as u32, self.pos as u32),
                "unterminated unicode escape; expected `}`",
            ));
        }
        let hex = &self.src[hex_start..self.pos - 1];
        if hex.is_empty() || hex.len() > 6 {
            return Err(LexError::new(
                Span::new(start as u32, self.pos as u32),
                "unicode escape must contain 1 to 6 hex digits",
            ));
        }
        let value = u32::from_str_radix(hex, 16).map_err(|_| {
            LexError::new(
                Span::new(start as u32, self.pos as u32),
                "invalid unicode escape",
            )
        })?;
        char::from_u32(value).ok_or_else(|| {
            LexError::new(
                Span::new(start as u32, self.pos as u32),
                "invalid unicode scalar value in escape",
            )
        })
    }

    /// Skip spaces, tabs, and comments. Returns whether a skipped block comment
    /// contained a newline (which acts as a statement separator).
    fn skip_space_and_comments(&mut self) -> Result<bool, LexError> {
        let mut newline_in_comment = false;
        loop {
            let before = self.pos;
            self.skip_horizontal_space();
            if self.peek() == Some('/') && self.peek2() == Some('/') {
                self.skip_line_comment();
                continue;
            }
            if self.peek() == Some('/') && self.peek2() == Some('*') {
                if self.skip_block_comment()? {
                    newline_in_comment = true;
                }
                continue;
            }
            if self.pos == before {
                break;
            }
        }
        Ok(newline_in_comment)
    }

    fn skip_horizontal_space(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.bump();
        }
    }

    fn skip_line_comment(&mut self) {
        self.bump(); // '/'
        self.bump(); // '/'
        while let Some(c) = self.peek() {
            if c == '\n' || c == '\r' {
                break;
            }
            self.bump();
        }
    }

    fn skip_block_comment(&mut self) -> Result<bool, LexError> {
        let start = self.pos;
        self.bump(); // '/'
        self.bump(); // '*'
        let mut depth = 1u32;
        let mut saw_newline = false;
        while depth > 0 {
            if self.is_eof() {
                return Err(LexError::new(
                    Span::new(start as u32, self.pos as u32),
                    "unterminated block comment",
                ));
            }
            if self.peek() == Some('/') && self.peek2() == Some('*') {
                self.bump();
                self.bump();
                depth += 1;
                continue;
            }
            if self.peek() == Some('*') && self.peek2() == Some('/') {
                self.bump();
                self.bump();
                depth -= 1;
                continue;
            }
            if matches!(self.peek(), Some('\n' | '\r')) {
                saw_newline = true;
            }
            self.bump();
        }
        Ok(saw_newline)
    }

    fn lex_compound(&mut self, start: usize, alone: TokenKind, next: char, both: TokenKind) {
        if self.eat(next) {
            self.push(both, start, self.pos);
        } else {
            self.push(alone, start, self.pos);
        }
    }

    fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        self.tokens
            .push(Token::new(kind, Span::new(start as u32, end as u32)));
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek2(&self) -> Option<char> {
        let mut chars = self.src[self.pos..].chars();
        chars.next()?;
        chars.next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn eat(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

fn strip_underscores(s: &str) -> String {
    s.chars().filter(|c| *c != '_').collect()
}

/// Render tokens as a compact debug listing (spans included).
#[must_use]
pub fn dump_tokens(source: &Source, tokens: &[Token]) -> String {
    let mut out = String::new();
    for token in tokens {
        let loc = source.locate(token.span.start);
        match &token.kind {
            TokenKind::Ident => {
                let name = source.slice(token.span);
                out.push_str(&format!(
                    "{:>4}:{}  ident({name})  @{}\n",
                    loc.line, loc.column, token.span
                ));
            }
            TokenKind::StringText(text) => {
                out.push_str(&format!(
                    "{:>4}:{}  string_text({text:?})  @{}\n",
                    loc.line, loc.column, token.span
                ));
            }
            other => {
                out.push_str(&format!(
                    "{:>4}:{}  {other}  @{}\n",
                    loc.line, loc.column, token.span
                ));
            }
        }
    }
    out
}
