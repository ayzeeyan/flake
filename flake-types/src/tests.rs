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
fn print_in_pure_function_is_rejected() {
    let msg = err(
        r#"
fn greet() / pure {
    print("hi")
}
fn main() { greet() }
"#,
    );
    assert!(msg.contains("io") || msg.contains("effects"), "{msg}");
}

#[test]
fn print_in_io_function_is_ok() {
    ok(r#"
fn greet() / io {
    print("hi")
}
fn main() { greet() }
"#);
}

#[test]
fn inferred_io_cannot_be_called_from_pure() {
    let msg = err(
        r#"
fn greet() {
    print("hi")
}
fn wrap() / pure {
    greet()
}
fn main() { wrap() }
"#,
    );
    assert!(msg.contains("effects") || msg.contains("io"), "{msg}");
}

#[test]
fn ordinary_code_allows_multiple_uses() {
    ok(r#"
fn take(x: owned String) {
    print(x)
    print(x)
}
fn main() { take("hi") }
"#);
}

#[test]
fn strict_owned_cannot_be_used_after_move() {
    let msg = err(
        r#"
strict fn take(x: owned String) {
    print(x)
    print(x)
}
fn main() { take("hi") }
"#,
    );
    assert!(msg.contains("moved"), "{msg}");
}

#[test]
fn strict_copy_types_can_be_reused() {
    ok(r#"
strict fn twice(x: Int) {
    print(x)
    print(x)
}
fn main() { twice(1) }
"#);
}

#[test]
fn strict_ref_can_be_reused() {
    ok(r#"
strict fn peek(x: ref String) {
    print(x)
    print(x)
}
fn main() { peek("hi") }
"#);
}

#[test]
fn cannot_assign_to_ref() {
    let msg = err(
        r#"
strict fn bump(x: ref String) {
    x = "no"
}
fn main() { bump("hi") }
"#,
    );
    assert!(msg.contains("ref"), "{msg}");
}

#[test]
fn read_file_requires_io_and_alloc() {
    let msg = err(
        r#"
fn load(path: String) -> String / io {
    read_file(path)
}
fn main() { }
"#,
    );
    assert!(msg.contains("alloc") || msg.contains("effects"), "{msg}");
}

#[test]
fn version_is_semver() {
    assert!(crate::version().contains('.'));
}
