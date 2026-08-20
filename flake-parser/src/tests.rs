use flake_ast::{
    BinOp, Expr, InterpPart, Item, Literal, Stmt, TypeExpr, print_program,
};

use crate::parse_str;

fn parse_ok(src: &str) -> flake_ast::Program {
    parse_str(src).unwrap_or_else(|e| panic!("parse failed for {src:?}: {e}"))
}

fn first_fn(src: &str) -> flake_ast::FnDecl {
    match parse_ok(src).items.into_iter().next() {
        Some(Item::Fn(f)) => f,
        other => panic!("expected function, got {other:?}"),
    }
}

#[test]
fn hello_example_parses() {
    let src = include_str!("../../examples/hello.flk");
    let program = crate::parse(&flake_ast::Source::new("examples/hello.flk", src))
        .expect("hello.flk should parse");
    assert_eq!(program.items.len(), 1);
    let f = match &program.items[0] {
        Item::Fn(f) => f,
        _ => panic!("expected fn main"),
    };
    assert_eq!(f.name.name, "main");
    assert_eq!(f.params.len(), 0);
    assert!(!f.effects.specified);
    match f.body.stmts.as_slice() {
        [Stmt::Let(s)] => {
            assert_eq!(s.name.name, "name");
            assert!(matches!(
                s.value,
                Expr::Literal {
                    value: Literal::String(ref v),
                    ..
                } if v == "World"
            ));
        }
        other => panic!("expected let name, got {other:?}"),
    }
    match f.body.tail.as_deref() {
        Some(Expr::Call { args, .. }) => {
            assert!(matches!(
                args.as_slice(),
                [Expr::Interpolated { parts, .. }]
                    if matches!(
                        parts.as_slice(),
                        [InterpPart::Text(a), InterpPart::Expr(Expr::Ident(id)), InterpPart::Text(b)]
                            if a == "Hello, " && id.name == "name" && b == "!"
                    )
            ));
        }
        other => panic!("expected print(...) tail, got {other:?}"),
    }
}

#[test]
fn function_with_effects_and_types() {
    let f = first_fn(
        "fn load_config(path: String) -> Config / io + alloc {\n    read_file(path)\n}",
    );
    assert_eq!(f.name.name, "load_config");
    assert_eq!(f.params.len(), 1);
    assert_eq!(f.params[0].name.name, "path");
    assert!(matches!(
        f.params[0].ty,
        Some(TypeExpr::Named { ref name, .. }) if name.name == "String"
    ));
    assert!(matches!(
        f.return_type,
        Some(TypeExpr::Named { ref name, .. }) if name.name == "Config"
    ));
    let names: Vec<_> = f.effects.names().collect();
    assert_eq!(names, ["io", "alloc"]);
}

#[test]
fn strict_owned_prefixes() {
    let f = first_fn("strict owned fn take(x: owned String) -> owned String { x }");
    assert!(f.strict);
    assert!(f.owned);
    assert!(matches!(f.params[0].ty, Some(TypeExpr::Owned { .. })));
}

#[test]
fn arithmetic_precedence() {
    let f = first_fn("fn f() { 1 + 2 * 3 }");
    match f.body.tail.as_deref() {
        Some(Expr::Binary {
            op: BinOp::Add,
            right,
            ..
        }) => {
            assert!(matches!(right.as_ref(), Expr::Binary { op: BinOp::Mul, .. }));
        }
        other => panic!("expected add of mul, got {other:?}"),
    }
}

#[test]
fn control_flow() {
    let src = r#"
fn f(n: Int) {
    if n > 0 {
        print("pos")
    } else if n == 0 {
        print("zero")
    } else {
        print("neg")
    }
    var i = 0
    while i < n {
        i = i + 1
    }
    for x in 0..n {
        print(x)
    }
    loop {
        break
    }
}
"#;
    let program = parse_ok(src);
    assert_eq!(program.items.len(), 1);
}

#[test]
fn lists_maps_and_index() {
    let f = first_fn(r#"fn f() { let xs = [1, 2, 3] xs[0] }"#);
    assert!(matches!(
        f.body.stmts.as_slice(),
        [Stmt::Let(s)] if matches!(s.value, Expr::List { .. })
    ));
    assert!(matches!(f.body.tail.as_deref(), Some(Expr::Index { .. })));
}

#[test]
fn map_literal() {
    let f = first_fn(r#"fn f() { { "a": 1, "b": 2 } }"#);
    assert!(matches!(f.body.tail.as_deref(), Some(Expr::Map { .. })));
}

#[test]
fn struct_decl_and_init() {
    let src = r#"
struct Config {
    host: String
    port: Int
}
fn f() {
    Config { host: "localhost", port: 80 }
}
"#;
    let program = parse_ok(src);
    assert!(matches!(program.items[0], Item::Struct(_)));
    match &program.items[1] {
        Item::Fn(f) => assert!(matches!(
            f.body.tail.as_deref(),
            Some(Expr::StructInit { .. })
        )),
        _ => panic!("expected fn"),
    }
}

#[test]
fn type_alias_and_dyn() {
    let src = "type Box = dyn\nfn f(x: dyn) -> [Int]? { x }";
    let program = parse_ok(src);
    assert!(matches!(program.items[0], Item::Type(_)));
    let f = match &program.items[1] {
        Item::Fn(f) => f,
        _ => panic!("fn"),
    };
    assert!(matches!(f.params[0].ty, Some(TypeExpr::Dyn { .. })));
    assert!(matches!(
        f.return_type,
        Some(TypeExpr::Optional { .. })
    ));
}

#[test]
fn assignment_and_compound() {
    let f = first_fn("fn f() { var x = 1 x += 2 x }");
    assert!(matches!(
        &f.body.stmts[..],
        [Stmt::Var(_), Stmt::Expr(Expr::Assign { .. })]
    ));
}

#[test]
fn newline_continues_binary_expr() {
    let f = first_fn("fn f() { 1 +\n 2 }");
    assert!(matches!(
        f.body.tail.as_deref(),
        Some(Expr::Binary { op: BinOp::Add, .. })
    ));
}

#[test]
fn pretty_print_contains_signature() {
    let src = "fn load_config(path: String) -> Config / io + alloc {\n    read_file(path)\n}\n";
    let program = parse_ok(src);
    let pretty = print_program(&program);
    assert!(pretty.contains("fn load_config(path: String) -> Config / io + alloc"));
    assert!(pretty.contains("read_file(path)"));
}

#[test]
fn pretty_roundtrip_hello() {
    let src = include_str!("../../examples/hello.flk");
    let program = crate::parse(&flake_ast::Source::new("hello.flk", src)).unwrap();
    let pretty = print_program(&program);
    let reparsed = parse_str(&pretty).expect("pretty-printed hello should parse");
    assert_eq!(reparsed.items.len(), program.items.len());
}

#[test]
fn error_on_bad_top_level() {
    let err = parse_str("1 + 2").unwrap_err();
    assert!(err.message.contains("expected"), "{}", err.message);
}

#[test]
fn error_on_unterminated_block() {
    let err = parse_str("fn main() {").unwrap_err();
    assert!(err.message.contains("`}`"), "{}", err.message);
}

#[test]
fn version_is_semver() {
    assert!(crate::version().contains('.'));
}
