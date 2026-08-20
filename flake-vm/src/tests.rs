use flake_ast::Source;

use crate::{Value, execute_captured};

fn run(src: &str) -> String {
    let source = Source::new("test.flk", src);
    execute_captured(&source)
        .unwrap_or_else(|e| panic!("vm failed:\n{}", e.display(&source)))
        .1
}

fn main(body: &str) -> String {
    format!("fn main() {{\n{body}\n}}\n")
}

#[test]
fn hello_world() {
    let src = include_str!("../../examples/hello.flk");
    assert_eq!(run(src), "Hello, World!\n");
}

#[test]
fn arithmetic() {
    assert_eq!(run(&main("print(1 + 2 * 3)")), "7\n");
}

#[test]
fn if_else() {
    assert_eq!(
        run(&main(r#"print(if true { "yes" } else { "no" })"#)),
        "yes\n"
    );
}

#[test]
fn while_loop() {
    let src = main(
        r#"
        var i = 0
        var s = 0
        while i < 4 {
            s = s + i
            i = i + 1
        }
        print(s)
        "#,
    );
    assert_eq!(run(&src), "6\n");
}

#[test]
fn function_call() {
    let src = r#"
fn add(a, b) {
    a + b
}
fn main() {
    print(add(2, 40))
}
"#;
    assert_eq!(run(src), "42\n");
}

#[test]
fn lists() {
    assert_eq!(run(&main("let xs = [1, 2, 3] print(xs[1])")), "2\n");
}

#[test]
fn version_is_semver() {
    assert!(crate::version().contains('.'));
}

#[test]
fn returns_nil_from_main() {
    let source = Source::new("t.flk", "fn main() {}");
    let (value, _) = execute_captured(&source).unwrap();
    assert_eq!(value, Value::Nil);
}
