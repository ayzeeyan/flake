use std::process::Command;

fn flake_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_flake"));
    cmd.env("NO_COLOR", "1");
    cmd
}

fn example(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("examples")
        .join(name)
}

fn run_example(name: &str) -> String {
    run_example_with(name, &[])
}

fn run_example_vm(name: &str) -> String {
    run_example_with(name, &["--vm"])
}

fn run_example_with(name: &str, extra: &[&str]) -> String {
    let mut cmd = flake_bin();
    cmd.arg("run");
    for flag in extra {
        cmd.arg(flag);
    }
    let output = cmd.arg(example(name)).output().expect("run example");
    assert!(
        output.status.success(),
        "{name} {extra:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn check_example(name: &str) {
    let output = flake_bin()
        .arg("check")
        .arg(example(name))
        .output()
        .expect("check example");
    assert!(
        output.status.success(),
        "{name} check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn all_examples_typecheck() {
    for name in [
        "hello.flk",
        "fibonacci.flk",
        "fizzbuzz.flk",
        "effects.flk",
        "lists.flk",
        "ownership.flk",
        "config.flk",
        "modules.flk",
        "stdlib.flk",
        "borrow.flk",
        "enum.flk",
        "visible.flk",
    ] {
        check_example(name);
    }
}

#[test]
fn hello_output() {
    assert_eq!(run_example("hello.flk"), "Hello, World!\n");
}

#[test]
fn fibonacci_output() {
    assert_eq!(run_example("fibonacci.flk"), "fib(10) = 55\n");
}

#[test]
fn fizzbuzz_output() {
    let out = run_example("fizzbuzz.flk");
    assert!(out.contains("FizzBuzz"), "{out}");
    assert!(out.contains("Fizz\n"), "{out}");
    assert!(out.contains("Buzz\n"), "{out}");
}

#[test]
fn effects_output() {
    let out = run_example("effects.flk");
    assert!(out.contains("Hello, Flake!"), "{out}");
    assert!(out.contains("2 + 2 = 4"), "{out}");
}

#[test]
fn lists_output() {
    let out = run_example("lists.flk");
    assert!(out.contains("sum = 15"), "{out}");
    assert!(out.contains("clarity, crystallized"), "{out}");
}

#[test]
fn ownership_output() {
    let out = run_example("ownership.flk");
    assert!(out.contains("consumed strict"), "{out}");
    assert!(out.contains("once: gradual"), "{out}");
    assert!(out.contains("twice: gradual"), "{out}");
}

#[test]
fn config_output() {
    assert_eq!(run_example("config.flk"), "listening on localhost:8080\n");
}

#[test]
fn modules_output() {
    assert_eq!(run_example("modules.flk"), "2 + 2 = 4\nsquare(5) = 25\n");
}

#[test]
fn stdlib_output() {
    let out = run_example("stdlib.flk");
    assert!(out.contains("first = 1 last = 3"), "{out}");
    assert!(out.contains("rest = [2, 3]"), "{out}");
    assert!(out.contains("true"), "{out}");
}

#[test]
fn borrow_output() {
    let out = run_example("borrow.flk");
    assert!(out.contains("borrowed Flake"), "{out}");
    assert!(out.contains("moved Flake"), "{out}");
}

#[test]
fn enum_output() {
    assert_eq!(run_example("enum.flk"), "red\nrgb 1,2,3\nok 42\nerr nope\n");
}

#[test]
fn visible_output() {
    assert_eq!(run_example("visible.flk"), "42\n42\n");
}

#[test]
fn native_matches_interpreter_on_all_examples() {
    for name in [
        "hello.flk",
        "fibonacci.flk",
        "fizzbuzz.flk",
        "effects.flk",
        "lists.flk",
        "ownership.flk",
        "config.flk",
        "modules.flk",
        "stdlib.flk",
        "borrow.flk",
        "enum.flk",
        "visible.flk",
    ] {
        let interp = run_example(name);
        let output = flake_bin()
            .arg("run")
            .arg("--native")
            .arg(example(name))
            .output()
            .expect("native");
        assert!(
            output.status.success(),
            "{name} native failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let native = String::from_utf8_lossy(&output.stdout);
        assert_eq!(interp, native, "{name}: interpreter and native diverged");
    }
}

#[test]
fn vm_matches_interpreter_on_all_examples() {
    for name in [
        "hello.flk",
        "fibonacci.flk",
        "fizzbuzz.flk",
        "effects.flk",
        "lists.flk",
        "ownership.flk",
        "config.flk",
        "modules.flk",
        "stdlib.flk",
        "borrow.flk",
        "enum.flk",
        "visible.flk",
    ] {
        let interp = run_example(name);
        let vm = run_example_vm(name);
        assert_eq!(interp, vm, "{name}: interpreter and VM diverged");
    }
}
