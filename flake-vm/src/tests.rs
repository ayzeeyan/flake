use flake_ast::Source;

use crate::{Value, execute_captured};

fn run(src: &str) -> String {
    let source = Source::new("test.flk", src);
    execute_captured(&source)
        .unwrap_or_else(|e| panic!("vm failed:\n{}", e.display(&source)))
        .1
}

fn run_err(src: &str) -> String {
    let source = Source::new("test.flk", src);
    execute_captured(&source)
        .expect_err("expected VM error")
        .display(&source)
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
fn for_range_and_break() {
    let src = main(
        r#"
        var s = 0
        for n in 0..10 {
            if n == 4 { break }
            s = s + n
        }
        print(s)
        "#,
    );
    assert_eq!(run(&src), "6\n");
}

#[test]
fn for_list_and_continue() {
    let src = main(
        r#"
        var s = 0
        for n in [1, 2, 3, 4] {
            if n == 2 { continue }
            s = s + n
        }
        print(s)
        "#,
    );
    assert_eq!(run(&src), "8\n");
}

#[test]
fn compound_assignment() {
    assert_eq!(run(&main("var x = 2 x += 3 print(x)")), "5\n");
}

#[test]
fn maps() {
    let src = main(
        r#"
        var m = { "a": 1, "b": 2 }
        m["c"] = 3
        print(m["a"])
        print(len(m))
        "#,
    );
    assert_eq!(run(&src), "1\n3\n");
}

#[test]
fn structs() {
    let src = r#"
struct Point { x: Int y: Int }
fn main() {
    var p = Point { x: 1, y: 2 }
    p.x += 7
    print(p.x)
    print(p.y)
}
"#;
    assert_eq!(run(src), "8\n2\n");
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
fn natives_join_range() {
    let src = main(
        r#"
        var s = 0
        for n in range(4) { s = s + n }
        print(s)
        print(join(["a", "b"], "-"))
        "#,
    );
    assert_eq!(run(&src), "6\na-b\n");
}

#[test]
fn loop_break() {
    assert_eq!(
        run(&main(
            r#"
            var i = 0
            loop {
                i += 1
                if i == 3 { break }
            }
            print(i)
            "#
        )),
        "3\n"
    );
}

#[test]
fn all_examples() {
    for (file, expect) in [
        ("hello.flk", "Hello, World!\n"),
        ("fibonacci.flk", "fib(10) = 55\n"),
        ("config.flk", "listening on localhost:8080\n"),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("examples")
            .join(file);
        let src = std::fs::read_to_string(&path).expect(file);
        let out = run(&src);
        assert_eq!(out, expect, "{file}");
    }
}

#[test]
fn enums_and_match() {
    let src = include_str!("../../examples/enum.flk");
    assert_eq!(run(src), "red\nrgb 1,2,3\nok 42\nerr nope\n");
}

#[test]
fn string_natives() {
    assert_eq!(
        run(&main(
            r#"print(trim("  hi  ")) print(upper("ab")) print(lower("AB"))"#
        )),
        "hi\nAB\nab\n"
    );
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

#[test]
fn structured_tasks_are_deferred_and_joined() {
    let src = r#"
fn work(n: Int) -> Int / io {
    print("work {n}")
    n * 2
}
fn main() / conc + io {
    let task = spawn work(21)
    print("spawned")
    print(await task)
}
"#;
    assert_eq!(run(src), "spawned\nwork 21\n42\n");
}

#[test]
fn unawaited_tasks_finish_before_scope_exit() {
    let src = r#"
fn child() / io { print("child") }
fn main() / conc + io {
    spawn child()
    print("parent")
}
"#;
    assert_eq!(run(src), "parent\nchild\n");
}

#[test]
fn nested_tasks_work() {
    let src = r#"
fn leaf(n: Int) -> Int { n + 1 }
fn branch(n: Int) -> Int / conc {
    let task = spawn leaf(n)
    await task
}
fn main() / conc + io {
    let task = spawn branch(41)
    print(await task)
}
"#;
    assert_eq!(run(src), "42\n");
}

#[test]
fn task_handles_can_only_be_awaited_once() {
    let message = run_err(
        r#"
fn work() -> Int { 42 }
fn main() / conc {
    let task = spawn work()
    await task
    await task
}
"#,
    );
    assert!(message.contains("already awaited"), "{message}");
}

#[test]
fn unawaited_task_failure_propagates() {
    let message = run_err(
        r#"
fn fail() -> Int { 1 / 0 }
fn main() / conc { spawn fail() }
"#,
    );
    assert!(message.contains("division by zero"), "{message}");
}

#[test]
fn spawned_enum_constructor_uses_ready_task() {
    let src = r#"
enum Result { Ok(Int) Err(String) }
fn main() / conc + io {
    let task = spawn Result.Ok(42)
    let result = await task
    print(match result {
        Result.Ok(value) => value
        Result.Err(message) => 0
    })
}
"#;
    assert_eq!(run(src), "42\n");
}
