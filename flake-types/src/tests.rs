use crate::check_str;

fn ok(src: &str) {
    check_str(src).unwrap_or_else(|e| panic!("check failed for {src:?}: {e}"));
}

fn err(src: &str) -> String {
    check_str(src)
        .expect_err("expected a type error")
        .to_string()
}

fn main(body: &str) -> String {
    format!("fn main() {{\n{body}\n}}\n")
}

#[test]
fn hello_checks() {
    let src = include_str!("../../examples/hello.flk");
    ok(src);
}

#[test]
fn infers_int_from_literal() {
    ok(&main("let x = 1 let y: Int = x"));
}

#[test]
fn rejects_bool_as_int() {
    let msg = err(&main("let x: Int = true"));
    assert!(msg.contains("type mismatch"), "{msg}");
}

#[test]
fn dyn_is_consistent_with_everything() {
    ok(&main(
        r#"
        let x: dyn = 1
        let y: dyn = true
        let z: Int = x
        print(y)
        "#,
    ));
}

#[test]
fn function_param_and_return() {
    ok(r#"
fn add(a: Int, b: Int) -> Int { a + b }
fn main() { print(add(1, 2)) }
"#);
}

#[test]
fn inferred_params_from_arithmetic() {
    ok(r#"
fn add(a, b) { a + b }
fn main() { print(add(2, 40)) }
"#);
}

#[test]
fn call_arity_error() {
    let msg = err(
        r#"
fn f(a: Int) { a }
fn main() { f() }
"#,
    );
    assert!(msg.contains("expected 1 argument"), "{msg}");
}

#[test]
fn if_condition_must_be_bool() {
    let msg = err(&main("if 1 { print(1) }"));
    assert!(msg.contains("type mismatch") || msg.contains("Bool"), "{msg}");
}

#[test]
fn list_homogeneity() {
    let msg = err(&main("let xs = [1, true]"));
    assert!(msg.contains("type mismatch"), "{msg}");
}

#[test]
fn list_of_dyn_ok() {
    ok(&main("let xs: [dyn] = [1, true]"));
}

#[test]
fn undefined_variable() {
    let msg = err(&main("print(nope)"));
    assert!(msg.contains("undefined"), "{msg}");
}

#[test]
fn version_is_semver() {
    assert!(crate::version().contains('.'));
}
