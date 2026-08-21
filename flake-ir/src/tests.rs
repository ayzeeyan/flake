use flake_ast::Source;

use crate::{Inst, lower, print_module};

fn ir(src: &str) -> String {
    let source = Source::new("t.flk", src);
    let module = lower(&source).unwrap_or_else(|e| panic!("{}", e.display(&source)));
    print_module(&module)
}

#[test]
fn lowers_add() {
    let dump = ir("fn add(a: Int, b: Int) -> Int { a + b }");
    assert!(dump.contains("fn add(a: Int, b: Int) -> Int"), "{dump}");
    assert!(dump.contains("add %"), "{dump}");
    assert!(dump.contains("return %"), "{dump}");
}

#[test]
fn lowers_hello() {
    let src = include_str!("../../examples/hello.flk");
    let source = Source::new("hello.flk", src);
    let module = lower(&source).unwrap();
    assert_eq!(module.functions.len(), 1);
    assert_eq!(module.functions[0].name, "main");
    let dump = print_module(&module);
    assert!(dump.contains("call print"), "{dump}");
    assert!(dump.contains("concat"), "{dump}");
}

#[test]
fn lowers_if_and_loop() {
    let dump = ir(r#"
fn f(n: Int) {
    if n > 0 { 1 } else { 0 }
}
"#);
    assert!(dump.contains("br %"), "{dump}");
    assert!(dump.contains("goto bb"), "{dump}");
}

#[test]
fn records_effects_and_strict() {
    let dump = ir("strict fn greet(name: String) / io { print(name) }");
    assert!(dump.contains("/ io"), "{dump}");
    assert!(dump.contains("strict"), "{dump}");
}

#[test]
fn every_block_ends_with_terminator() {
    let src = include_str!("../../examples/fizzbuzz.flk");
    let source = Source::new("fizzbuzz.flk", src);
    let module = lower(&source).unwrap();
    for func in &module.functions {
        for block in &func.blocks {
            assert!(
                block.insts.last().is_some_and(Inst::is_terminator),
                "bb{} in {} has no terminator: {:?}",
                block.id.0,
                func.name,
                block.insts
            );
        }
    }
}

#[test]
fn lowers_enum_match() {
    let dump = ir(r#"
enum Color { Red Green }
fn f(c: Color) -> Int {
    match c {
        Color.Red => 1
        Color.Green => 2
    }
}
"#);
    assert!(dump.contains("[%"), "{dump}");
    assert!(dump.contains("br %"), "{dump}");
}

#[test]
fn version_is_semver() {
    assert!(crate::version().contains('.'));
}
