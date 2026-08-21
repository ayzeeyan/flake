use std::process::Command;

fn flake_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_flake"));
    cmd.env("NO_COLOR", "1");
    cmd
}

#[test]
fn version_flag_prints_semver() {
    let output = flake_bin().arg("--version").output().expect("run flake");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("flake"), "stdout: {stdout}");
    assert!(stdout.contains("0.2.0"), "stdout: {stdout}");
}

#[test]
fn help_flag_lists_core_commands() {
    let output = flake_bin().arg("--help").output().expect("run flake");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Clarity, crystallized"), "stdout: {stdout}");
    assert!(stdout.contains("run"), "stdout: {stdout}");
    assert!(stdout.contains("check"), "stdout: {stdout}");
    assert!(stdout.contains("repl"), "stdout: {stdout}");
}

#[test]
fn run_without_file_fails() {
    let status = flake_bin().arg("run").status().expect("run flake");
    assert!(!status.success());
}

#[test]
fn check_hello_example() {
    let hello = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("examples")
        .join("hello.flk");
    let output = flake_bin()
        .arg("check")
        .arg(&hello)
        .output()
        .expect("check hello");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ok");
}

#[test]
fn run_hello_example_vm() {
    let hello = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("examples")
        .join("hello.flk");
    let output = flake_bin()
        .arg("run")
        .arg("--vm")
        .arg(&hello)
        .output()
        .expect("run hello on vm");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Hello, World!\n");
}

#[test]
fn run_hello_example_native() {
    let hello = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("examples")
        .join("hello.flk");
    let output = flake_bin()
        .arg("run")
        .arg("--native")
        .arg(&hello)
        .output()
        .expect("run hello native");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Hello, World!\n");
}

#[test]
fn run_hello_example() {
    let hello = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("examples")
        .join("hello.flk");
    let output = flake_bin()
        .arg("run")
        .arg(&hello)
        .output()
        .expect("run hello");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Hello, World!\n");
}
