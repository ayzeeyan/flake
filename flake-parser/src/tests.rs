use flake_ast::{BinOp, Expr, InterpPart, Item, Literal, Stmt, TypeExpr, print_program};

use crate::{Lockfile, Manifest, ReplInput, parse_repl, parse_str};

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
    let f =
        first_fn("fn load_config(path: String) -> Config / io + alloc {\n    read_file(path)\n}");
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
            assert!(matches!(
                right.as_ref(),
                Expr::Binary { op: BinOp::Mul, .. }
            ));
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
    assert!(matches!(f.return_type, Some(TypeExpr::Optional { .. })));
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
fn dotted_imports_parse_and_pretty_print() {
    let program = parse_ok("import services.checkout as checkout\nfn main() {}");
    let Item::Import(import) = &program.items[0] else {
        panic!("expected import");
    };
    assert_eq!(import.path.name, "services.checkout");
    assert_eq!(
        import.alias.as_ref().map(|alias| alias.name.as_str()),
        Some("checkout")
    );
    let pretty = print_program(&program);
    assert!(
        pretty.contains("import services.checkout as checkout"),
        "{pretty}"
    );
    parse_str(&pretty).expect("pretty-printed dotted import should parse");
}

#[test]
fn qualified_types_and_variant_patterns_parse() {
    let program = parse_ok(
        "fn label(value: domain.Status) -> Int { match value { domain.Status.Ready => 1 _ => 0 } }",
    );
    let Item::Fn(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert!(matches!(
        function.params[0].ty,
        Some(TypeExpr::Named { ref name, .. }) if name.name == "domain.Status"
    ));
    let Some(Expr::Match { arms, .. }) = function.body.tail.as_deref() else {
        panic!("expected match");
    };
    assert!(matches!(
        &arms[0].pattern,
        flake_ast::Pattern::Variant { ty: Some(name), variant, .. }
            if name.name == "domain.Status" && variant.name == "Ready"
    ));
    let pretty = print_program(&program);
    assert!(pretty.contains("domain.Status.Ready"), "{pretty}");
    parse_str(&pretty).expect("pretty-printed qualified pattern should parse");
}

#[test]
fn qualified_struct_initializers_parse() {
    let program = parse_ok("fn point() -> geometry.Point { geometry.Point { x: 3, y: 4 } }");
    let Item::Fn(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert!(matches!(
        function.body.tail.as_deref(),
        Some(Expr::StructInit { name, fields, .. })
            if name.name == "geometry.Point" && fields.len() == 2
    ));
    let pretty = print_program(&program);
    assert!(pretty.contains("geometry.Point {"), "{pretty}");
    parse_str(&pretty).expect("pretty-printed qualified struct should parse");
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
fn repl_parses_bare_expression() {
    let src = flake_ast::Source::new("<repl>", "1 + 2");
    match parse_repl(&src).unwrap() {
        ReplInput::Script(block) => assert!(block.tail.is_some()),
        other => panic!("expected script, got {other:?}"),
    }
}

#[test]
fn enum_and_match_parse() {
    let src = r#"
enum Color {
    Red
    Rgb(Int, Int, Int)
}
fn describe(c: Color) -> String {
    match c {
        Color.Red => "red"
        Color.Rgb(r, g, b) => "rgb"
        _ => "other"
    }
}
"#;
    let program = parse_ok(src);
    assert!(matches!(program.items[0], Item::Enum(_)));
    let f = match &program.items[1] {
        Item::Fn(f) => f,
        other => panic!("expected fn, got {other:?}"),
    };
    assert!(matches!(f.body.tail.as_deref(), Some(Expr::Match { .. })));
    let pretty = print_program(&program);
    assert!(pretty.contains("enum Color"), "{pretty}");
    assert!(pretty.contains("match "), "{pretty}");
    parse_str(&pretty).expect("pretty-printed enum/match should parse");
}

#[test]
fn pub_enum_parses() {
    let program = parse_ok("pub enum Flag { On Off }");
    match &program.items[0] {
        Item::Enum(e) => {
            assert!(e.is_pub);
            assert_eq!(e.variants.len(), 2);
        }
        other => panic!("expected enum, got {other:?}"),
    }
}

#[test]
fn spawn_and_await_parse_and_pretty_print() {
    let program = parse_ok(
        "fn work(n: Int) -> Int { n + 1 }\nfn main() / conc { let task: Task[Int] = spawn work(41) await task }",
    );
    let Item::Fn(main) = &program.items[1] else {
        panic!("expected main function");
    };
    assert!(matches!(
        main.body.stmts.as_slice(),
        [Stmt::Let(s)] if matches!(s.value, Expr::Spawn { .. })
    ));
    assert!(matches!(
        main.body.tail.as_deref(),
        Some(Expr::Await { .. })
    ));
    let pretty = print_program(&program);
    assert!(pretty.contains("spawn work(41)"), "{pretty}");
    assert!(pretty.contains("await task"), "{pretty}");
    parse_str(&pretty).expect("pretty-printed concurrency should parse");
}

#[test]
fn spawn_requires_a_call() {
    let err = parse_str("fn main() / conc { spawn 42 }").unwrap_err();
    assert!(err.message.contains("function call"), "{}", err.message);
}

#[test]
fn result_try_and_literal_patterns_parse() {
    let program = parse_ok(
        r#"
enum Result { Ok(Int) Err(String) }
fn parse() -> Result { Result.Ok(42) }
fn use_it() -> Result {
    let value = parse()?
    match value {
        -1 => Result.Err("negative")
        42 => Result.Ok(value)
        _ => Result.Err("other")
    }
}
"#,
    );
    let Item::Fn(use_it) = &program.items[2] else {
        panic!("expected use_it function");
    };
    assert!(matches!(
        use_it.body.stmts.as_slice(),
        [Stmt::Let(s)] if matches!(s.value, Expr::Try { .. })
    ));
    let Some(Expr::Match { arms, .. }) = use_it.body.tail.as_deref() else {
        panic!("expected match tail");
    };
    assert!(matches!(
        arms[0].pattern,
        flake_ast::Pattern::Literal {
            value: Literal::Int(-1),
            ..
        }
    ));
    let pretty = print_program(&program);
    assert!(pretty.contains("parse()?"), "{pretty}");
    assert!(pretty.contains("-1 =>"), "{pretty}");
    parse_str(&pretty).expect("pretty-printed result flow should parse");
}

#[test]
fn pub_import_parses() {
    let program = parse_ok("pub import utils.math as m\npub import service");
    assert_eq!(program.items.len(), 2);
    let Item::Import(ref i1) = program.items[0] else { panic!("expected import"); };
    assert!(i1.is_pub);
    assert_eq!(i1.path.name, "utils.math");
    assert_eq!(i1.alias.as_ref().map(|a| a.name.as_str()), Some("m"));

    let Item::Import(ref i2) = program.items[1] else { panic!("expected import"); };
    assert!(i2.is_pub);
    assert_eq!(i2.path.name, "service");
    assert_eq!(i2.alias, None);
}

#[test]
fn repl_recognizes_enum_items() {
    let src = flake_ast::Source::new("<repl>", "enum Flag { On Off }");
    assert!(matches!(parse_repl(&src).unwrap(), ReplInput::Program(_)));
}

#[test]
fn nursery_block_parses() {
    let program = parse_ok("fn main() / conc { nursery { let t = spawn work() await t } }");
    let Item::Fn(f) = &program.items[0] else { panic!("expected function"); };
    assert_eq!(f.body.stmts.len(), 0);
    let Some(Expr::Nursery { body, .. }) = f.body.tail.as_deref() else {
        panic!("expected nursery expression");
    };
    assert_eq!(body.stmts.len(), 1);
    assert!(body.tail.is_some());
    let pretty = print_program(&program);
    assert!(pretty.contains("nursery {"), "{pretty}");
}

#[test]
fn lockfile_generation_and_roundtrip() {
    let dir = std::env::temp_dir().join(format!("flake-lock-test-{}", std::process::id()));
    let lib_dir = dir.join("lib");
    std::fs::create_dir_all(&lib_dir).unwrap();

    let lib_manifest_text = r#"
[package]
name = "math_lib"
version = "0.2.0"
"#;
    std::fs::write(lib_dir.join("flake.toml"), lib_manifest_text).unwrap();
    std::fs::write(lib_dir.join("main.flk"), "pub fn add(a: Int, b: Int) -> Int { a + b }\n").unwrap();

    let app_manifest_text = r#"
[package]
name = "my_app"
version = "1.0.0"

[dependencies]
math = { path = "lib", package = "math_lib", version = "0.2.0" }
"#;
    let app_manifest_path = dir.join("flake.toml");
    std::fs::write(&app_manifest_path, app_manifest_text).unwrap();
    std::fs::write(dir.join("main.flk"), "import math\nfn main() { print(math.add(1, 2)) }\n").unwrap();

    let manifest = Manifest::parse(app_manifest_text, &app_manifest_path).unwrap();
    let lockfile = Lockfile::generate(&manifest, &dir).unwrap();

    assert_eq!(lockfile.root_package, "my_app");
    assert_eq!(lockfile.packages.len(), 2);
    assert_eq!(lockfile.packages[0].name, "math_lib");
    assert_eq!(lockfile.packages[1].name, "my_app");

    let toml = lockfile.to_toml_string();
    assert!(toml.contains("lockfile_version = 1"));
    assert!(toml.contains("root_package = \"my_app\""));
    assert!(toml.contains("name = \"math_lib\""));

    let parsed = Lockfile::parse(&toml, &dir.join("flake.lock")).unwrap();
    assert_eq!(parsed.root_package, lockfile.root_package);
    assert_eq!(parsed.packages.len(), lockfile.packages.len());
    assert_eq!(parsed.packages[0].name, lockfile.packages[0].name);

    lockfile.verify(&manifest, &dir).expect("lockfile should verify against manifest");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn version_is_semver() {
    assert!(crate::version().contains('.'));
}
