use flake_ast::Source;

use crate::{Engine, Value, execute_captured};

fn run(src: &str) -> (Value, String) {
    let source = Source::new("test.flk", src);
    execute_captured(&source).unwrap_or_else(|e| panic!("run failed:\n{}", e.display(&source)))
}

fn run_err(src: &str) -> String {
    let source = Source::new("test.flk", src);
    execute_captured(&source)
        .expect_err("expected a runtime/parse error")
        .display(&source)
}

fn main(body: &str) -> String {
    format!("fn main() {{\n{body}\n}}\n")
}

#[test]
fn hello_world() {
    let src = include_str!("../../examples/hello.flk");
    let (_, out) = run(src);
    assert_eq!(out, "Hello, World!\n");
}

#[test]
fn arithmetic_and_print() {
    let (_, out) = run(&main(r#"print(1 + 2 * 3) print(10 / 2) print(7 % 3)"#));
    assert_eq!(out, "7\n5\n1\n");
}

#[test]
fn variables_and_assignment() {
    let (_, out) = run(&main(
        r#"
        let a = 1
        var b = 2
        b = b + a
        print(b)
        "#,
    ));
    assert_eq!(out, "3\n");
}

#[test]
fn immutable_assign_errors() {
    let msg = run_err(&main("let x = 1 x = 2"));
    assert!(msg.contains("immutable"), "{msg}");
}

#[test]
fn undefined_variable_errors() {
    let msg = run_err(&main("print(nope)"));
    assert!(msg.contains("undefined variable"), "{msg}");
}

#[test]
fn functions_and_return() {
    let src = r#"
fn add(a, b) {
    a + b
}
fn main() {
    print(add(2, 40))
}
"#;
    let (_, out) = run(src);
    assert_eq!(out, "42\n");
}

#[test]
fn recursion() {
    let src = r#"
fn fib(n) {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}
fn main() {
    print(fib(8))
}
"#;
    let (_, out) = run(src);
    assert_eq!(out, "21\n");
}

#[test]
fn if_else_expression() {
    let (_, out) = run(&main(r#"print(if true { "yes" } else { "no" })"#));
    assert_eq!(out, "yes\n");
}

#[test]
fn while_and_for() {
    let (_, out) = run(&main(
        r#"
        var s = 0
        var i = 0
        while i < 5 {
            s = s + i
            i = i + 1
        }
        print(s)
        var t = 0
        for n in 0..5 {
            t = t + n
        }
        print(t)
        "#,
    ));
    assert_eq!(out, "10\n10\n");
}

#[test]
fn lists_len_push_index() {
    let (_, out) = run(&main(
        r#"
        let xs = [1, 2]
        push(xs, 3)
        print(len(xs))
        print(xs[0])
        print(xs[-1])
        "#,
    ));
    assert_eq!(out, "3\n1\n3\n");
}

#[test]
fn interpolation_with_int() {
    let (_, out) = run(&main(r#"let n = 3 print("n={n}")"#));
    assert_eq!(out, "n=3\n");
}

#[test]
fn division_by_zero() {
    let msg = run_err(&main("print(1 / 0)"));
    assert!(msg.contains("division by zero"), "{msg}");
}

#[test]
fn type_error_on_add() {
    let msg = run_err(&main(r#"print(true + 1)"#));
    assert!(msg.contains("cannot add"), "{msg}");
}

#[test]
fn arity_error() {
    let src = r#"
fn f(a) { a }
fn main() { f() }
"#;
    let msg = run_err(src);
    assert!(msg.contains("expected 1 argument"), "{msg}");
}

#[test]
fn short_circuit() {
    let src = r#"
fn boom() {
    print("nope")
    true
}
fn main() {
    print(false && boom())
    print(true || boom())
}
"#;
    let (_, out) = run(src);
    assert_eq!(out, "false\ntrue\n");
}

#[test]
fn struct_fields() {
    let src = r#"
struct Point { x: Int y: Int }
fn main() {
    var p = Point { x: 1, y: 2 }
    p.x = 8
    print(p.x)
    print(p.y)
}
"#;
    let (_, out) = run(src);
    assert_eq!(out, "8\n2\n");
}

#[test]
fn no_main_errors() {
    let msg = run_err("fn helper() { 1 }");
    assert!(msg.contains("no `main`"), "{msg}");
}

#[test]
fn range_join_split() {
    let (_, out) = run(&main(
        r#"
        var s = 0
        for n in range(4) {
            s = s + n
        }
        print(s)
        print(join(["a", "b"], "-"))
        print(len(split("a-b-c", "-")))
        "#,
    ));
    assert_eq!(out, "6\na-b\n3\n");
}

#[test]
fn repl_state_persists() {
    let mut engine = Engine::new();
    let mut out = Vec::new();
    engine
        .eval_repl(&Source::new("<repl>", "let x = 40"), &mut out)
        .unwrap();
    let value = engine
        .eval_repl(&Source::new("<repl>", "x + 2"), &mut out)
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn stdlib_natives() {
    let (_, out) = run(&main(
        r#"
        print(first([9, 8]))
        print(last("ab"))
        print(starts_with("flake", "fl"))
        print(contains([1, 2], 2))
        "#,
    ));
    assert_eq!(out, "9\nb\ntrue\ntrue\n");
}

#[test]
fn write_file_roundtrip() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("flake-interp-write-{}.txt", std::process::id()));
    let posix = path.to_string_lossy().replace('\\', "/");
    let (_, out) = run(&main(&format!(
        r#"write_file("{posix}", "hello") print(read_file("{posix}"))"#
    )));
    let _ = std::fs::remove_file(&path);
    assert_eq!(out, "hello\n");
}

#[test]
fn enums_and_match() {
    let src = include_str!("../../examples/enum.flk");
    let (_, out) = run(src);
    assert_eq!(out, "red\nrgb 1,2,3\nok 42\nerr nope\n");
}

#[test]
fn enum_equality() {
    let (_, out) = run(r#"
enum Color { Red Green }
fn main() {
    print(Color.Red == Color.Red)
    print(Color.Red == Color.Green)
}
"#);
    assert_eq!(out, "true\nfalse\n");
}

#[test]
fn version_is_semver() {
    assert!(crate::version().contains('.'));
}
