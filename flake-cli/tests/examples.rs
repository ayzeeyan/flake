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
    let output = flake_bin()
        .arg("run")
        .arg(example(name))
        .output()
        .expect("run example");
    assert!(
        output.status.success(),
        "{name} failed:\n{}",
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
