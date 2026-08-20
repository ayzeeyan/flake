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
    assert!(stdout.contains("0.1.0"), "stdout: {stdout}");
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
