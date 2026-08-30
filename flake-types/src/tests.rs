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
    let msg = err(r#"
fn f(a: Int) { a }
fn main() { f() }
"#);
    assert!(msg.contains("expected 1 argument"), "{msg}");
}

#[test]
fn if_condition_must_be_bool() {
    let msg = err(&main("if 1 { print(1) }"));
    assert!(
        msg.contains("type mismatch") || msg.contains("Bool"),
        "{msg}"
    );
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
    let msg = err(r#"
fn greet() / pure {
    print("hi")
}
fn main() { greet() }
"#);
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
    let msg = err(r#"
fn greet() {
    print("hi")
}
fn wrap() / pure {
    greet()
}
fn main() { wrap() }
"#);
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
    let msg = err(r#"
strict fn take(x: owned String) {
    print(x)
    print(x)
}
fn main() { take("hi") }
"#);
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
    let msg = err(r#"
strict fn bump(x: ref String) {
    x = "no"
}
fn main() { bump("hi") }
"#);
    assert!(msg.contains("ref"), "{msg}");
}

#[test]
fn read_file_requires_io_and_alloc() {
    let msg = err(r#"
fn load(path: String) -> String / io {
    read_file(path)
}
fn main() { }
"#);
    assert!(msg.contains("alloc") || msg.contains("effects"), "{msg}");
}

#[test]
fn reinit_after_move_is_allowed() {
    ok(r#"
strict fn f() {
    var x: owned String = "a"
    print(x)
    x = "b"
    print(x)
}
fn main() { f() }
"#);
}

#[test]
fn cannot_move_while_borrowed() {
    let msg = err(r#"
strict fn f() {
    let x: owned String = "hi"
    let r = &x
    print(x)
    print(r)
}
fn main() { f() }
"#);
    assert!(msg.contains("borrow"), "{msg}");
}

#[test]
fn exclusive_mut_borrow() {
    let msg = err(r#"
strict fn f() {
    var x: owned String = "hi"
    let a = &mut x
    let b = &mut x
    print(a)
    print(b)
}
fn main() { f() }
"#);
    assert!(msg.contains("borrow"), "{msg}");
}

#[test]
fn temp_borrow_ends_after_statement() {
    ok(r#"
strict fn f() {
    let x: owned String = "hi"
    print(&x)
    print(x)
}
fn main() { f() }
"#);
}

#[test]
fn cannot_assign_while_shared_borrow() {
    let msg = err(r#"
strict fn f() {
    var x: owned String = "hi"
    let r = &x
    x = "no"
    print(r)
}
fn main() { f() }
"#);
    assert!(msg.contains("borrow"), "{msg}");
}

#[test]
fn borrow_ends_at_end_of_block() {
    ok(r#"
strict fn f() {
    let x: owned String = "hi"
    {
        let r = &x
        print(r)
    }
    print(x)
}
fn main() { f() }
"#);
}

#[test]
fn cannot_move_owned_inside_loop() {
    let msg = err(r#"
strict fn f() {
    let x: owned String = "hi"
    loop {
        print(x)
    }
}
fn main() { f() }
"#);
    assert!(msg.contains("loop") || msg.contains("moved"), "{msg}");
}

#[test]
fn if_else_move_both_branches_then_unusable() {
    let msg = err(r#"
strict fn f(b: Bool) {
    let x: owned String = "hi"
    if b {
        print(x)
    } else {
        print(x)
    }
    print(x)
}
fn main() { f(true) }
"#);
    assert!(msg.contains("moved"), "{msg}");
}

#[test]
fn enum_and_match_check() {
    ok(r#"
enum Color { Red Green Rgb(Int, Int, Int) }
fn f(c: Color) -> Int {
    match c {
        Color.Red => 1
        Color.Green => 2
        Color.Rgb(r, g, b) => r + g + b
    }
}
fn main() { print(f(Color.Red)) }
"#);
}

#[test]
fn match_must_be_exhaustive() {
    let msg = err(r#"
enum Color { Red Green }
fn f(c: Color) -> Int {
    match c {
        Color.Red => 1
    }
}
fn main() { f(Color.Red) }
"#);
    assert!(msg.contains("non-exhaustive"), "{msg}");
    assert!(msg.contains("Green"), "{msg}");
    assert!(msg.contains("help:"), "{msg}");
}

#[test]
fn unknown_variant_lists_alternatives() {
    let msg = err(r#"
enum Color { Red Green }
fn main() { print(Color.Blue) }
"#);
    assert!(msg.contains("no variant"), "{msg}");
    assert!(msg.contains("Red"), "{msg}");
    assert!(msg.contains("Green"), "{msg}");
}

#[test]
fn undefined_suggests_similar_name() {
    let msg = err(&main("print(prnt(1))"));
    assert!(msg.contains("undefined"), "{msg}");
    assert!(msg.contains("print"), "{msg}");
}

#[test]
fn variant_arity_must_match() {
    let msg = err(r#"
enum Color { Rgb(Int, Int, Int) }
fn f(c: Color) -> Int {
    match c {
        Color.Rgb(r) => r
    }
}
fn main() { f(Color.Rgb(1, 2, 3)) }
"#);
    assert!(msg.contains("expects 3"), "{msg}");
}

#[test]
fn result_try_propagates_the_ok_type() {
    ok(r#"
enum Result { Ok(Int) Err(String) }
fn source(ok: Bool) -> Result {
    if ok { Result.Ok(40) } else { Result.Err("no value") }
}
fn add_two(ok: Bool) -> Result {
    let value: Int = source(ok)?
    Result.Ok(value + 2)
}
fn main() { add_two(true) }
"#);
}

#[test]
fn result_try_requires_a_result_return_type() {
    let msg = err(r#"
enum Result { Ok(Int) Err(String) }
fn source() -> Result { Result.Ok(1) }
fn bad() -> Int { source()? }
fn main() { bad() }
"#);
    assert!(msg.contains("propagate"), "{msg}");
}

#[test]
fn literal_patterns_check_and_bool_match_is_exhaustive() {
    ok(r#"
fn choose(flag: Bool) -> Int {
    match flag {
        true => 1
        false => 0
    }
}
fn main() { choose(true) }
"#);
}

#[test]
fn duplicate_and_unreachable_match_arms_are_rejected() {
    let duplicate = err(r#"
fn f(n: Int) -> Int {
    match n { 1 => 1 1 => 2 _ => 0 }
}
fn main() { f(1) }
"#);
    assert!(duplicate.contains("duplicate"), "{duplicate}");

    let unreachable = err(r#"
fn f(n: Int) -> Int {
    match n { _ => 0 1 => 1 }
}
fn main() { f(1) }
"#);
    assert!(unreachable.contains("unreachable"), "{unreachable}");
}

#[test]
fn enum_patterns_must_match_the_scrutinee_enum() {
    let msg = err(r#"
enum Color { Red }
enum Shape { Circle }
fn f(color: Color) -> Int {
    match color { Shape.Circle => 1 }
}
fn main() { f(Color.Red) }
"#);
    assert!(msg.contains("type mismatch"), "{msg}");
}

#[test]
fn map_keys_are_limited_to_stable_scalar_types() {
    let msg = err(&main(
        "let bad: Map[[Int], String] = { \"key\": \"value\" }",
    ));
    assert!(msg.contains("map key"), "{msg}");
    assert!(msg.contains("String, Int, or Bool"), "{msg}");
}

#[test]
fn duplicate_enum_variants_are_rejected() {
    let msg = err("enum Flag { On On }\nfn main() {}");
    assert!(msg.contains("duplicate variant"), "{msg}");
}

#[test]
fn explicit_returns_are_checked_against_the_function_type() {
    ok("fn answer() -> Int { return 42 }\nfn main() { answer() }");
    let msg = err("fn bad() -> Int { return \"no\" }\nfn main() { bad() }");
    assert!(msg.contains("type mismatch"), "{msg}");
}

#[test]
fn private_fn_is_not_imported() {
    let dir = std::env::temp_dir().join(format!("flake-vis-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::write(
        dir.join("lib.flk"),
        "pub fn ok() -> Int { 1 }\nfn secret() -> Int { 2 }\n",
    )
    .expect("write lib");
    let main_path = dir.join("main.flk");
    let text = "import lib\nfn main() { lib.secret() }\n";
    std::fs::write(&main_path, text).expect("write main");
    let source = flake_ast::Source::new(main_path.display().to_string(), text);
    let err = crate::check(&source).expect_err("private import should fail");
    let msg = err.to_string();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(msg.contains("export") || msg.contains("secret"), "{msg}");
}

#[test]
fn modules_are_private_by_default_even_without_public_items() {
    let dir = std::env::temp_dir().join(format!(
        "flake-private-default-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::write(dir.join("lib.flk"), "fn hidden() -> Int { 42 }\n").expect("write lib");
    let main_path = dir.join("main.flk");
    let text = "import lib\nfn main() { lib.hidden() }\n";
    std::fs::write(&main_path, text).expect("write main");
    let source = flake_ast::Source::new(main_path.display().to_string(), text);
    let error = crate::check(&source).expect_err("unmarked declaration should be private");
    let message = error.to_string();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(message.contains("no export `hidden`"), "{message}");
}

#[test]
fn public_apis_cannot_leak_private_types() {
    let message = err(
        "struct Secret { code: Int }\npub fn reveal(secret: Secret) -> Int { secret.code }\nfn main() {}",
    );
    assert!(
        message.contains("exposes private type `Secret`"),
        "{message}"
    );
    assert!(message.contains("mark `Secret` `pub`"), "{message}");
}

#[test]
fn colliding_imports_require_qualified_names() {
    let dir = std::env::temp_dir().join(format!(
        "flake-import-collision-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("left")).expect("left dir");
    std::fs::create_dir_all(dir.join("right")).expect("right dir");
    std::fs::write(
        dir.join("left/value.flk"),
        "pub enum Status { Ready }\npub fn value() -> Int { 1 }\n",
    )
    .expect("write left");
    std::fs::write(
        dir.join("right/value.flk"),
        "pub enum Status { Ready }\npub fn value() -> Int { 2 }\n",
    )
    .expect("write right");
    let main_path = dir.join("main.flk");
    let qualified = "import left.value as left\nimport right.value as right\nfn describe(status: left.Status) -> Int { match status { left.Status.Ready => 1 } }\nfn main() { describe(left.Status.Ready) + left.value() + right.value() }\n";
    std::fs::write(&main_path, qualified).expect("write qualified main");
    let source = flake_ast::Source::new(main_path.display().to_string(), qualified);
    crate::check(&source).expect("qualified imports should type-check");

    let mismatched = "import left.value as left\nimport right.value as right\nfn wrong(status: left.Status) -> right.Status { status }\nfn main() {}\n";
    std::fs::write(&main_path, mismatched).expect("write nominal mismatch");
    let source = flake_ast::Source::new(main_path.display().to_string(), mismatched);
    let error = crate::check(&source).expect_err("module-qualified enum types are nominal");
    assert!(error.to_string().contains("type mismatch"), "{error}");

    let ambiguous =
        "import left.value as left\nimport right.value as right\nfn main() { value() }\n";
    std::fs::write(&main_path, ambiguous).expect("write ambiguous main");
    let source = flake_ast::Source::new(main_path.display().to_string(), ambiguous);
    let error = crate::check(&source).expect_err("bare collision should fail");
    let message = error.to_string();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        message.contains("ambiguous imported name `value`"),
        "{message}"
    );
    assert!(message.contains("left.value"), "{message}");
    assert!(message.contains("right.value"), "{message}");
}

#[test]
fn spawn_and_await_have_typed_task_results() {
    ok(r#"
fn work(n: Int) -> Int { n + 1 }
fn main() / conc {
    let task: Task[Int] = spawn work(41)
    let answer: Int = await task
}
"#);
}

#[test]
fn spawn_requires_conc_effect() {
    let msg = err(r#"
fn work() -> Int { 42 }
fn wrapper() / pure {
    let task = spawn work()
    await task
}
fn main() { wrapper() }
"#);
    assert!(msg.contains("conc"), "{msg}");
}

#[test]
fn spawned_child_effects_are_preserved() {
    let msg = err(r#"
fn noisy() / io { print("child") }
fn wrapper() / conc {
    let task = spawn noisy()
    await task
}
fn main() { wrapper() }
"#);
    assert!(msg.contains("io"), "{msg}");
}

#[test]
fn await_requires_a_task() {
    let msg = err("fn main() / conc { await 42 }");
    assert!(msg.contains("cannot await Int"), "{msg}");
}

#[test]
fn task_handles_cannot_escape_their_function() {
    let msg = err(r#"
fn work() -> Int { 42 }
fn leak() / conc {
    let task = spawn work()
    task
}
fn main() { }
"#);
    assert!(msg.contains("cannot escape"), "{msg}");

    let msg = err(r#"
fn work() -> Int { 42 }
fn leak() / conc {
    let task = spawn work()
    return task
}
fn main() { }
"#);
    assert!(msg.contains("cannot escape"), "{msg}");
}

#[test]
fn overloaded_builtins_check_supported_forms_and_preserve_numeric_types() {
    ok(r#"
fn main() {
    print()
    print(1, "two", true)
    assert(true)
    assert(true, "still true")
    let one_arg = range(3)
    let two_args = range(1, 3)
    let integer: Int = abs(-4)
    let decimal: Float = abs(-1.5)
    let low: Int = min(3, 1, 2)
    let high: Float = max(1.0, 3.5, 2.0)
    print(one_arg, two_args, integer, decimal, low, high)
}
"#);
}

#[test]
fn overloaded_builtins_report_bad_arity_during_checking() {
    for (source, expected) in [
        ("fn main() { assert() }", "expected 1 or 2"),
        (
            "fn main() { assert(true, \"ok\", \"extra\") }",
            "expected 1 or 2",
        ),
        ("fn main() { range() }", "expected 1 or 2"),
        ("fn main() { range(1, 2, 3) }", "expected 1 or 2"),
        ("fn main() { min(1) }", "expected at least 2"),
        ("fn main() { abs(1, 2) }", "expected 1 argument"),
    ] {
        let message = err(source);
        assert!(message.contains(expected), "{message}");
        assert!(message.contains("help:"), "{message}");
    }
}

#[test]
fn overloaded_builtins_reject_backend_ambiguous_types() {
    for source in [
        "fn main() { assert(1) }",
        "fn main() { assert(true, 42) }",
        "fn main() { range(1.5) }",
        "fn main() { abs(\"no\") }",
        "fn main() { min(1, 2.0) }",
        "fn main() { max(true, false) }",
    ] {
        let message = err(source);
        assert!(
            message.contains("type mismatch")
                || message.contains("expected Int or Float")
                || message.contains("String"),
            "{message}"
        );
    }
}

#[test]
fn remainder_requires_homogeneous_numeric_operands() {
    ok("fn main() { let integer: Int = 7 % 3 let decimal: Float = 7.5 % 2.0 }");
    let message = err("fn main() { print(7 % 2.0) }");
    assert!(message.contains("type mismatch"), "{message}");
}

#[test]
fn nested_and_list_patterns_check() {
    ok(r#"
enum Option {
    Some(Int)
    None
}

enum Result {
    Ok(Option)
    Err(String)
}

fn handle(res: Result) -> Int {
    match res {
        Result.Ok(Option.Some(n)) => n
        Result.Ok(Option.None) => 0
        Result.Err(_) => -1
    }
}

fn handle_list(xs: [Int]) -> Int {
    match xs {
        [a, b] => a + b
        _ => 0
    }
}

fn main() {
    let r = Result.Ok(Option.Some(42))
    print(handle(r))
    print(handle_list([10, 20]))
}
"#);
}

#[test]
fn pattern_exhaustiveness_and_mismatch_diagnostics() {
    let message = err(r#"
enum Color { Red Green Blue }
fn check(c: Color) -> String {
    match c {
        Color.Red => "red"
        Color.Green => "green"
    }
}
fn main() {}
"#);
    assert!(
        message.contains("non-exhaustive match on `Color`: missing Blue"),
        "{message}"
    );

    let message = err(r#"
enum Color { Red Green Blue }
fn check(c: Color) -> String {
    match c {
        Color.Red => "red"
        _ => "other"
        Color.Blue => "blue"
    }
}
fn main() {}
"#);
    assert!(message.contains("unreachable match arm"), "{message}");
}

#[test]
fn structural_borrow_conflicts_prevent_moves() {
    let message = err(r#"
struct Pair { x: String, y: String }
fn consume(p: owned Pair) {}
fn inspect(s: ref String) {}

strict fn f() {
    let p = Pair { x: "a", y: "b" }
    let r = &p.x
    consume(p)
}
"#);
    assert!(
        message.contains("cannot move `p` while it is borrowed"),
        "{message}"
    );
}

#[test]
fn structural_borrow_conflicts_prevent_field_mutation() {
    let message = err(r#"
struct Pair { x: String, y: String }

strict fn f() {
    var p = Pair { x: "a", y: "b" }
    let r = &p.x
    p.y = "new"
}
"#);
    assert!(
        message.contains("cannot assign to field of `p` while it is borrowed"),
        "{message}"
    );
}

#[test]
fn structural_borrow_conflicts_prevent_field_mutation_when_moved() {
    let message = err(r#"
struct Pair { x: String, y: String }
fn consume(p: owned Pair) {}

strict fn f() {
    var p = Pair { x: "a", y: "b" }
    consume(p)
    p.x = "new"
}
"#);
    assert!(
        message.contains("cannot assign to field of `p` because it was already moved"),
        "{message}"
    );
}

#[test]
fn match_arms_branch_aware_move_checking() {
    let message = err(r#"
enum Option { Some(String) None }
fn consume(s: owned String) {}

strict fn f() {
    let opt = Option.Some("data")
    let s = "hello"
    match opt {
        Option.Some(val) => consume(s)
        Option.None => consume(s)
    }
    // Since both arms moved s, using s here must fail
    consume(s)
}
"#);
    assert!(message.contains("use of moved value `s`"), "{message}");
}

#[test]
fn task_handles_cannot_escape_nursery_via_outer_assignment() {
    let msg = err(r#"
fn work() -> Int { 42 }
fn f() / conc {
    var outer: Task[Int] = spawn work()
    let ignored = await outer
    nursery {
        let t = spawn work()
        outer = t
    }
}
fn main() { }
"#);
    assert!(
        msg.contains("cannot assign task handle to variable defined outside the nursery")
            || msg.contains("task handle cannot escape"),
        "{msg}"
    );
}

#[test]
fn task_handles_cannot_escape_nursery_via_block_value() {
    let msg = err(r#"
fn work() -> Int { 42 }
fn f() / conc {
    let escaped = nursery {
        let t = spawn work()
        t
    }
}
fn main() { }
"#);
    assert!(
        msg.contains("task handle cannot escape its nursery"),
        "{msg}"
    );
}

#[test]
fn valid_nursery_typechecks_cleanly() {
    ok(r#"
fn work(n: Int) -> Int { n * 2 }
fn f() -> Int / conc {
    nursery {
        let t1 = spawn work(10)
        let t2 = spawn work(20)
        let r1 = await t1
        let r2 = await t2
        r1 + r2
    }
}
fn main() {}
"#);
}

#[test]
fn concurrency_task_inspection_builtins_typecheck() {
    ok(r#"
fn work() -> Int { 42 }
fn test_inspection() / conc + io {
    let t = spawn work()
    let status: String = task_status(t)
    let completed: Bool = is_completed(t)
    let cancelled: Bool = is_cancelled(t)
    let res = await t
}
fn main() {}
"#);
}

#[test]
fn spawn_rejects_borrowed_reference_argument() {
    let msg = err(r#"
fn work(x: ref String) -> String { x }
fn caller() / conc {
    let s = "hello"
    let t = spawn work(&s)
}
fn main() {}
"#);
    assert!(
        msg.contains("cannot capture reference across task boundary"),
        "{msg}"
    );
}

#[test]
fn strict_spawn_rejects_ref_parameter() {
    let msg = err(r#"
fn work(s: String) -> String { s }
strict fn caller(r: ref String) / conc {
    let t = spawn work(r)
}
fn main() {}
"#);
    assert!(
        msg.contains("cannot capture reference `r` across task boundary"),
        "{msg}"
    );
}

#[test]
fn strict_match_arm_pattern_bindings_track_ownership() {
    let msg = err(r#"
enum Option { Some(String) None }
fn consume(s: owned String) {}

strict fn f(opt: Option) {
    match opt {
        Option.Some(val) => {
            consume(val)
            consume(val)
        }
        Option.None => nil
    }
}
fn main() {}
"#);
    assert!(msg.contains("use of moved value `val`"), "{msg}");
}

#[test]
fn ownership_diagnostics_and_field_sensitive_moves() {
    let msg = err(r#"
fn consume(s: owned String) {}

strict fn test(s: owned String) {
    consume(s)
    consume(s)
}
fn main() {}
"#);
    assert!(msg.contains("use of moved value `s`"), "{msg}");
}


#[test]
fn spawn_rejects_nested_reference_arguments() {
    let msg = err(r#"
fn work(items: [ref String]) / conc {}
fn main() / conc {
    let s = "hello"
    let list = [&s]
    let t = spawn work(list)
}
"#);
    assert!(
        msg.contains("cannot capture reference `list` across task boundary into `spawn`"),
        "{msg}"
    );
}

#[test]
fn spawn_rejects_generic_struct_with_reference() {
    let msg = err(r#"
struct Wrapper[T] {
    val: T,
}
fn work(w: Wrapper[ref String]) / conc {}
strict fn main() / conc {
    let s = "hello"
    let w = Wrapper { val: &s }
    let t = spawn work(&s)
}
"#);
    assert!(
        msg.contains("cannot capture reference"),
        "{msg}"
    );
}


#[test]
fn strict_match_arms_are_isolated_branches() {
    let text = r#"
enum Either { Left(String) Right(String) }
fn consume(s: owned String) {}

strict fn process(e: Either) {
    match e {
        Either.Left(s) => {
            consume(s)
        }
        Either.Right(s) => {
            consume(s)
        }
    }
}
fn main() {}
"#;
    ok(text);
}

#[test]
fn structural_borrow_prevents_moving_root_struct() {
    let msg = err(r#"
struct Point { x: String, y: String }
fn consume(p: owned Point) {}

strict fn test() {
    let p = Point { x: "10", y: "20" }
    let r = &p.x
    consume(p)
}
fn main() {}
"#);
    assert!(
        msg.contains("cannot move `p` while it is borrowed"),
        "{msg}"
    );
}

#[test]
fn generic_function_identity_and_pair() {
    ok(r#"
fn id[T](x: T) -> T {
    x
}

fn make_pair[A, B](first: A, second: B) -> Pair[A, B] {
    Pair { first: first, second: second }
}

struct Pair[A, B] {
    first: A,
    second: B,
}

fn main() {
    let a: Int = id(42)
    let b: String = id("hello")
    let p: Pair[Int, String] = make_pair(10, "flake")
}
"#);
}

#[test]
fn generic_enum_and_pattern_matching() {
    ok(r#"
enum Option[T] {
    Some(T)
    None
}

fn unwrap_or[T](opt: Option[T], default_val: T) -> T {
    match opt {
        Option.Some(val) => val,
        Option.None => default_val,
    }
}

fn main() {
    let s: Option[Int] = Option.Some(100)
    let res: Int = unwrap_or(s, 0)
    let n: Option[String] = Option.None
    let s_res: String = unwrap_or(n, "fallback")
}
"#);
}

#[test]
fn generic_type_alias() {
    ok(r#"
enum Result[T, E] {
    Ok(T)
    Err(E)
}

type IoResult[T] = Result[T, String]

fn produce() -> IoResult[Int] {
    Result.Ok(42)
}

fn main() {
    let res: IoResult[Int] = produce()
}
"#);
}

#[test]
fn generic_type_arity_mismatch() {
    let msg = err(r#"
struct Pair[A, B] {
    first: A,
    second: B,
}

fn test(p: Pair[Int]) {}
fn main() {}
"#);
    assert!(msg.contains("expects 2 type argument(s), got 1"), "{msg}");
}

#[test]
fn local_reference_escape_is_rejected() {
    let msg = err(r#"
strict fn escape_local() -> ref String {
    let s = "local value"
    return &s
}
"#);
    assert!(
        msg.contains("cannot return reference to local variable `s`"),
        "{msg}"
    );
}

#[test]
fn generic_nested_containers_and_pattern_matching() {
    let code = r#"
enum Option[T] {
    Some(T),
    None,
}

enum Container[T] {
    Item(T),
    Empty,
}

fn unwrap_or[T](opt: Option[T], fallback: T) -> T {
    match opt {
        Option.Some(val) => val,
        Option.None => fallback,
    }
}

fn extract_container[T](c: Container[Option[T]], fallback: T) -> T {
    match c {
        Container.Item(opt) => unwrap_or(opt, fallback),
        Container.Empty => fallback,
    }
}

fn main() {
    let c = Container.Item(Option.Some(42))
    let res = extract_container(c, 0)
    assert(res == 42)
}
"#;
    ok(code);
}

#[test]
fn generic_higher_order_functions() {
    let code = r#"
struct Pair[A, B] {
    first: A,
    second: B,
}

fn map_pair[A, B, C](p: Pair[A, B], f: fn(A) -> C) -> Pair[C, B] {
    Pair {
        first: f(p.first),
        second: p.second,
    }
}

fn add_one(x: Int) -> Int { x + 1 }

fn main() {
    let p = Pair { first: 10, second: "hello" }
    let p2 = map_pair(p, add_one)
    assert(p2.first == 11)
}
"#;
    ok(code);
}

#[test]
fn version_is_semver() {
    assert!(crate::version().contains('.'));
}


