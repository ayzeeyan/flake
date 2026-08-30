use flake_ast::Source;

use crate::token::TokenKind;
use crate::{tokenize, tokenize_str};

fn kinds(src: &str) -> Vec<TokenKind> {
    tokenize_str(src)
        .unwrap_or_else(|e| panic!("lex error for {src:?}: {e}"))
        .into_iter()
        .map(|t| t.kind)
        .collect()
}

fn kinds_no_nl(src: &str) -> Vec<TokenKind> {
    kinds(src)
        .into_iter()
        .filter(|k| !matches!(k, TokenKind::Newline | TokenKind::Eof))
        .collect()
}

fn ident() -> TokenKind {
    TokenKind::Ident
}

fn text(s: &str) -> TokenKind {
    TokenKind::StringText(s.to_string())
}

#[test]
fn empty_source_is_just_eof() {
    assert_eq!(kinds(""), vec![TokenKind::Eof]);
}

#[test]
fn whitespace_only_is_eof() {
    assert_eq!(kinds("  \t  "), vec![TokenKind::Eof]);
}

#[test]
fn keywords_and_identifiers() {
    let src = "fn let var if else while for loop in return break continue true false nil dyn type struct enum strict owned ref mut import as pub unsafe match spawn await nursery trait impl foo _bar Baz_1";
    let got = kinds_no_nl(src);
    assert_eq!(
        got,
        vec![
            TokenKind::Fn,
            TokenKind::Let,
            TokenKind::Var,
            TokenKind::If,
            TokenKind::Else,
            TokenKind::While,
            TokenKind::For,
            TokenKind::Loop,
            TokenKind::In,
            TokenKind::Return,
            TokenKind::Break,
            TokenKind::Continue,
            TokenKind::True,
            TokenKind::False,
            TokenKind::Nil,
            TokenKind::Dyn,
            TokenKind::Type,
            TokenKind::Struct,
            TokenKind::Enum,
            TokenKind::Strict,
            TokenKind::Owned,
            TokenKind::Ref,
            TokenKind::Mut,
            TokenKind::Import,
            TokenKind::As,
            TokenKind::Pub,
            TokenKind::Unsafe,
            TokenKind::Match,
            TokenKind::Spawn,
            TokenKind::Await,
            TokenKind::Nursery,
            TokenKind::Trait,
            TokenKind::Impl,
            ident(),
            ident(),
            ident(),
        ]
    );
}

#[test]
fn identifier_is_not_keyword_prefix() {
    assert_eq!(kinds_no_nl("fnord lettuce"), vec![ident(), ident()]);
}

#[test]
fn integers_decimal_hex_bin_oct() {
    assert_eq!(
        kinds_no_nl("0 42 1_000 0xFF 0b1010 0o17"),
        vec![
            TokenKind::Int(0),
            TokenKind::Int(42),
            TokenKind::Int(1000),
            TokenKind::Int(255),
            TokenKind::Int(10),
            TokenKind::Int(15),
        ]
    );
}

#[test]
fn range_is_not_a_float() {
    assert_eq!(
        kinds_no_nl("1..10"),
        vec![TokenKind::Int(1), TokenKind::DotDot, TokenKind::Int(10)]
    );
}

#[test]
fn floats_and_scientific() {
    let got = kinds_no_nl("3.14 0.5 .25 1e3 1.5e-2");
    match &got[..] {
        [
            TokenKind::Float(a),
            TokenKind::Float(b),
            TokenKind::Float(c),
            TokenKind::Float(d),
            TokenKind::Float(e),
        ] => {
            let expected = "3.14".parse::<f64>().unwrap();
            assert!((a - expected).abs() < 1e-10);
            assert!((b - 0.5).abs() < 1e-10);
            assert!((c - 0.25).abs() < 1e-10);
            assert!((d - 1e3).abs() < 1e-10);
            assert!((e - 1.5e-2).abs() < 1e-10);
        }
        other => panic!("unexpected tokens: {other:?}"),
    }
}

#[test]
fn int_dot_ident_is_not_float() {
    assert_eq!(
        kinds_no_nl("1.abs"),
        vec![TokenKind::Int(1), TokenKind::Dot, ident()]
    );
}

#[test]
fn simple_string() {
    assert_eq!(
        kinds_no_nl(r#""hello""#),
        vec![TokenKind::StringStart, text("hello"), TokenKind::StringEnd,]
    );
}

#[test]
fn empty_string() {
    assert_eq!(
        kinds_no_nl(r#""""#),
        vec![TokenKind::StringStart, TokenKind::StringEnd]
    );
}

#[test]
fn string_escapes() {
    assert_eq!(
        kinds_no_nl(r#""a\n\t\r\\\"\{}""#),
        vec![
            TokenKind::StringStart,
            text("a\n\t\r\\\"{}"),
            TokenKind::StringEnd,
        ]
    );
}

#[test]
fn unicode_escape() {
    assert_eq!(
        kinds_no_nl(r#""\u{1F31F}""#),
        vec![TokenKind::StringStart, text("🌟"), TokenKind::StringEnd,]
    );
}

#[test]
fn string_interpolation() {
    assert_eq!(
        kinds_no_nl(r#""Hello, {name}!""#),
        vec![
            TokenKind::StringStart,
            text("Hello, "),
            TokenKind::InterpOpen,
            ident(),
            TokenKind::InterpClose,
            text("!"),
            TokenKind::StringEnd,
        ]
    );
}

#[test]
fn interpolation_with_expression() {
    assert_eq!(
        kinds_no_nl(r#""count: {x + 1}""#),
        vec![
            TokenKind::StringStart,
            text("count: "),
            TokenKind::InterpOpen,
            ident(),
            TokenKind::Plus,
            TokenKind::Int(1),
            TokenKind::InterpClose,
            TokenKind::StringEnd,
        ]
    );
}

#[test]
fn nested_string_inside_interpolation() {
    assert_eq!(
        kinds_no_nl(r#""a {foo("b {y}")} c""#),
        vec![
            TokenKind::StringStart,
            text("a "),
            TokenKind::InterpOpen,
            ident(),
            TokenKind::LParen,
            TokenKind::StringStart,
            text("b "),
            TokenKind::InterpOpen,
            ident(),
            TokenKind::InterpClose,
            TokenKind::StringEnd,
            TokenKind::RParen,
            TokenKind::InterpClose,
            text(" c"),
            TokenKind::StringEnd,
        ]
    );
}

#[test]
fn interpolation_with_nested_braces() {
    assert_eq!(
        kinds_no_nl(r#""{ {1} }""#),
        vec![
            TokenKind::StringStart,
            TokenKind::InterpOpen,
            TokenKind::LBrace,
            TokenKind::Int(1),
            TokenKind::RBrace,
            TokenKind::InterpClose,
            TokenKind::StringEnd,
        ]
    );
}

#[test]
fn line_and_block_comments_are_discarded() {
    let src = "a // line\nb /* block */ c /* nest /* inner */ out */ d";
    assert_eq!(kinds_no_nl(src), vec![ident(), ident(), ident(), ident()]);
}

#[test]
fn nested_block_comments() {
    assert_eq!(kinds_no_nl("x /* a /* b */ c */ y"), vec![ident(), ident()]);
}

#[test]
fn block_comment_with_newline_separates_statements() {
    let got = kinds("let x = 1 /*\n*/ let y = 2");
    assert!(
        got.contains(&TokenKind::Newline),
        "expected a newline token from the block comment, got {got:?}"
    );
}

#[test]
fn operators_and_punctuation() {
    let src = "+ - * / % == != < > <= >= && || ! = += -= *= /= %= -> => & .. ? ( ) { } [ ] , : ;";
    assert_eq!(
        kinds_no_nl(src),
        vec![
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
            TokenKind::EqEq,
            TokenKind::BangEq,
            TokenKind::Lt,
            TokenKind::Gt,
            TokenKind::LtEq,
            TokenKind::GtEq,
            TokenKind::AmpAmp,
            TokenKind::PipePipe,
            TokenKind::Bang,
            TokenKind::Eq,
            TokenKind::PlusEq,
            TokenKind::MinusEq,
            TokenKind::StarEq,
            TokenKind::SlashEq,
            TokenKind::PercentEq,
            TokenKind::Arrow,
            TokenKind::FatArrow,
            TokenKind::Amp,
            TokenKind::DotDot,
            TokenKind::Question,
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::LBracket,
            TokenKind::RBracket,
            TokenKind::Comma,
            TokenKind::Colon,
            TokenKind::Semicolon,
        ]
    );
}

#[test]
fn effect_signature_tokens() {
    assert_eq!(
        kinds_no_nl("fn read_file(path: String) -> String / io + alloc"),
        vec![
            TokenKind::Fn,
            ident(),
            TokenKind::LParen,
            ident(),
            TokenKind::Colon,
            ident(),
            TokenKind::RParen,
            TokenKind::Arrow,
            ident(),
            TokenKind::Slash,
            ident(),
            TokenKind::Plus,
            ident(),
        ]
    );
}

#[test]
fn newlines_are_tokens() {
    let got = kinds("a\nb\n\nc");
    assert_eq!(
        got,
        vec![
            ident(),
            TokenKind::Newline,
            ident(),
            TokenKind::Newline,
            ident(),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn crlf_counts_as_one_newline() {
    let got = kinds("a\r\nb");
    assert_eq!(
        got,
        vec![ident(), TokenKind::Newline, ident(), TokenKind::Eof]
    );
}

#[test]
fn hello_example_lexes() {
    let src = include_str!("../../examples/hello.flk");
    let source = Source::new("examples/hello.flk", src);
    let tokens = tokenize(&source).expect("hello.flk should lex");
    let kinds: Vec<_> = tokens
        .iter()
        .map(|t| t.kind.clone())
        .filter(|k| !matches!(k, TokenKind::Newline | TokenKind::Eof))
        .collect();
    assert_eq!(
        kinds,
        vec![
            TokenKind::Fn,
            ident(),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::Let,
            ident(),
            TokenKind::Eq,
            TokenKind::StringStart,
            text("World"),
            TokenKind::StringEnd,
            ident(),
            TokenKind::LParen,
            TokenKind::StringStart,
            text("Hello, "),
            TokenKind::InterpOpen,
            ident(),
            TokenKind::InterpClose,
            text("!"),
            TokenKind::StringEnd,
            TokenKind::RParen,
            TokenKind::RBrace,
        ]
    );
}

#[test]
fn token_spans_cover_lexeme() {
    let src = "let x = 42";
    let source = Source::new("t.flk", src);
    let tokens = tokenize(&source).unwrap();
    assert_eq!(source.slice(tokens[0].span), "let");
    assert_eq!(source.slice(tokens[1].span), "x");
    assert_eq!(source.slice(tokens[2].span), "=");
    assert_eq!(source.slice(tokens[3].span), "42");
}

#[test]
fn unterminated_string_errors() {
    let err = tokenize_str("\"abc").unwrap_err();
    assert!(err.message.contains("unterminated string"));
}

#[test]
fn unterminated_block_comment_errors() {
    let err = tokenize_str("/* oops").unwrap_err();
    assert!(err.message.contains("unterminated block comment"));
}

#[test]
fn unknown_escape_errors() {
    let err = tokenize_str(r#""\q""#).unwrap_err();
    assert!(err.message.contains("unknown string escape"));
}

#[test]
fn unexpected_character_errors() {
    let err = tokenize_str("@").unwrap_err();
    assert!(err.message.contains("unexpected character"));
}

#[test]
fn single_pipe_errors() {
    let err = tokenize_str("a | b").unwrap_err();
    assert!(err.message.contains("||"));
}

#[test]
fn integer_overflow_errors() {
    let err = tokenize_str("9223372036854775808").unwrap_err();
    assert!(err.message.contains("out of range"));
}

#[test]
fn trailing_underscore_in_number_errors() {
    let err = tokenize_str("1_").unwrap_err();
    assert!(err.message.contains("_"));
}

#[test]
fn hex_without_digits_errors() {
    let err = tokenize_str("0x").unwrap_err();
    assert!(err.message.contains("hexadecimal"));
}

#[test]
fn newline_in_string_errors() {
    let err = tokenize_str("\"hi\nthere\"").unwrap_err();
    assert!(err.message.contains("unterminated string"));
}

#[test]
fn unterminated_interpolation_errors() {
    let err = tokenize_str("\"hello {name\"").unwrap_err();
    assert!(
        err.message.contains("unterminated"),
        "message: {}",
        err.message
    );
}

#[test]
fn version_is_semver() {
    assert!(crate::version().contains('.'));
}
