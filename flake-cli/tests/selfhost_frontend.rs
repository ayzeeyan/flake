use std::path::Path;
use std::process::Command;

fn flake_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_flake"));
    cmd.env("NO_COLOR", "1");
    cmd
}

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

fn selfhost_main() -> std::path::PathBuf {
    repo_root()
        .join("selfhost")
        .join("frontend")
        .join("main.flk")
}

fn run_selfhost(args: &[&str], vm: bool) -> (bool, String) {
    let mut cmd = flake_bin();
    cmd.current_dir(repo_root());
    cmd.arg("run");
    if vm {
        cmd.arg("--vm");
    }
    cmd.arg(selfhost_main());
    cmd.arg("--");
    for a in args {
        cmd.arg(a);
    }
    let output = cmd.output().expect("failed to execute flake command");
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");
    let combined = format!("{stdout}{stderr}");
    (output.status.success(), combined)
}

#[test]
fn selfhost_tokens_hello() {
    for vm in [false, true] {
        let (ok, out) = run_selfhost(&["--tokens", "examples/hello.flk"], vm);
        assert!(ok, "failed on vm={vm}: {out}");
        assert!(out.contains("fn \"fn\""));
        assert!(out.contains("Ident(main) \"main\""));
        assert!(out.contains("let \"let\""));
        assert!(out.contains("Ident(name) \"name\""));
        assert!(out.contains("String(\"World\") \"World\""));
        assert!(out.contains("Ident(print) \"print\""));
        assert!(out.contains("<eof> \"\""));
    }
}

#[test]
fn selfhost_check_examples() {
    for vm in [false, true] {
        let (ok, out) = run_selfhost(
            &[
                "--check",
                "examples/hello.flk",
                "examples/fibonacci.flk",
                "examples/enum.flk",
                "examples/traits.flk",
                "examples/ast_show.flk",
            ],
            vm,
        );
        assert!(ok, "failed on vm={vm}: {out}");
        assert!(out.contains("ok: examples/hello.flk"));
        assert!(out.contains("ok: examples/fibonacci.flk"));
        assert!(out.contains("ok: examples/enum.flk"));
        assert!(out.contains("ok: examples/traits.flk"));
        assert!(out.contains("ok: examples/ast_show.flk"));
    }
}

#[test]
fn selfhost_ast_golden_hello() {
    let expected = "\
(program
  (fn main () (block (let name \"World\") (call print \"Hello, {name}!\")))
)\n";
    for vm in [false, true] {
        let (ok, out) = run_selfhost(&["--ast", "examples/hello.flk"], vm);
        assert!(ok, "failed on vm={vm}: {out}");
        assert_eq!(out, expected, "AST mismatch on vm={vm}");
    }
}

#[test]
fn selfhost_ast_golden_enum() {
    let expected = "\
(program
  (enum Color (Red) (Green) (Rgb Int Int Int))
  (enum Result (Ok Int) (Err String))
  (fn describe (c:Color) (block (match c (arm Color.Red \"red\") (arm Color.Green \"green\") (arm Color.Rgb(r, g, b) \"rgb {r},{g},{b}\"))))
  (fn label (r:Result) (block (match r (arm Result.Ok(v) \"ok {v}\") (arm Result.Err(m) \"err {m}\"))))
  (fn main () (block (call print (call describe (field Color Red))) (call print (call describe (call (field Color Rgb) 1 2 3))) (call print (call label (call (field Result Ok) 42))) (call print (call label (call (field Result Err) \"nope\")))))
)\n";
    for vm in [false, true] {
        let (ok, out) = run_selfhost(&["--ast", "examples/enum.flk"], vm);
        assert!(ok, "failed on vm={vm}: {out}");
        assert_eq!(out, expected, "AST mismatch on vm={vm}");
    }
}

#[test]
fn selfhost_ast_golden_traits() {
    let expected = "\
(program
  (trait Show (fn show (self) -> String))
  (impl Show for Int (fn show (self) (block (call str self))))
  (impl Show for String (fn show (self) (block self)))
  (struct Pair[T: Eq] (left T) (right T))
  (impl Show for Pair (fn show (self) (block \"Pair({self.left}, {self.right})\")))
  (fn max[T: Ord] (a:T, b:T) (block (if (> a b) (block a) (block b))))
  (fn same[T: Eq] (a:T, b:T) (block (== a b)))
  (fn display[T: Show] (value:T) (block (call (field value show))))
  (fn main () (block (call print (call max 3 9)) (call print (call max \"alpha\" \"zeta\")) (call print (call same 7 7)) (call print (call same \"flake\" \"flake\")) (call print (call display 42)) (call print (call display \"crystallized\")) (let p (struct-init Pair (left 1) (right 2))) (call print (call (field p show)))))
)\n";
    for vm in [false, true] {
        let (ok, out) = run_selfhost(&["--ast", "examples/traits.flk"], vm);
        assert!(ok, "failed on vm={vm}: {out}");
        assert_eq!(out, expected, "AST mismatch on vm={vm}");
    }
}

#[test]
fn selfhost_walk_all_examples() {
    for vm in [false, true] {
        let (ok, out) = run_selfhost(&["--walk", "examples"], vm);
        assert!(ok, "failed on vm={vm}: {out}");
        assert!(
            out.contains("Scanned 60 files: all parsed successfully"),
            "unexpected output on vm={vm}: {out}"
        );
    }
}

#[test]
fn selfhost_walk_frontend_self_parsing() {
    let (ok, out) = run_selfhost(&["--walk", "selfhost/frontend"], true);
    assert!(ok, "failed selfhost walk on VM: {out}");
    assert!(
        out.contains("Scanned 6 files: all parsed successfully"),
        "unexpected output: {out}"
    );
}

#[test]
fn selfhost_native_check() {
    let mut cmd = flake_bin();
    cmd.current_dir(repo_root());
    cmd.arg("run");
    cmd.arg("--native");
    cmd.arg(selfhost_main());
    cmd.arg("--");
    cmd.arg("--check");
    cmd.arg("examples/hello.flk");
    let output = cmd.output().expect("failed to execute native flake run");
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");
    let combined = format!("{stdout}{stderr}");
    assert!(output.status.success(), "native check failed: {combined}");
    assert!(combined.contains("ok: examples/hello.flk"));
}
