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
    assert!(stdout.contains("0.3.0"), "stdout: {stdout}");
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
fn match_error_includes_help() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("flake-match-help-{}.flk", std::process::id()));
    std::fs::write(
        &path,
        "enum Color { Red Green }\nfn main() { match Color.Red { Color.Red => 1 } }\n",
    )
    .unwrap();
    let output = flake_bin().arg("check").arg(&path).output().expect("check");
    let _ = std::fs::remove_file(&path);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("non-exhaustive") || stderr.contains("Green"),
        "{stderr}"
    );
    assert!(stderr.contains("help") || stderr.contains("_"), "{stderr}");
}

#[test]
fn missing_import_reports_module_name() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("flake-missing-import-{}.flk", std::process::id()));
    std::fs::write(&path, "import definitely_missing\nfn main() {}\n").unwrap();
    let output = flake_bin().arg("check").arg(&path).output().expect("check");
    let _ = std::fs::remove_file(&path);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("definitely_missing") || stderr.contains("cannot find module"),
        "{stderr}"
    );
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
