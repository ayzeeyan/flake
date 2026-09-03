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
                "examples/effects.flk",
                "examples/ownership.flk",
                "examples/borrow.flk",
                "examples/nursery.flk",
            ],
            vm,
        );
        assert!(ok, "failed on vm={vm}: {out}");
        assert!(out.contains("ok: examples/hello.flk"));
        assert!(out.contains("ok: examples/fibonacci.flk"));
        assert!(out.contains("ok: examples/enum.flk"));
        assert!(out.contains("ok: examples/traits.flk"));
        assert!(out.contains("ok: examples/ast_show.flk"));
        assert!(out.contains("ok: examples/effects.flk"));
        assert!(out.contains("ok: examples/ownership.flk"));
        assert!(out.contains("ok: examples/borrow.flk"));
        assert!(out.contains("ok: examples/nursery.flk"));
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
            out.contains("Scanned 61 files: all parsed successfully"),
            "unexpected output on vm={vm}: {out}"
        );
    }
}

#[test]
fn selfhost_walk_frontend_self_parsing() {
    let (ok, out) = run_selfhost(&["--walk", "selfhost/frontend"], true);
    assert!(ok, "failed selfhost walk on VM: {out}");
    assert!(
        out.contains("all parsed successfully"),
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
    cmd.arg("examples/effects.flk");
    cmd.arg("examples/ownership.flk");
    cmd.arg("examples/borrow.flk");
    cmd.arg("examples/nursery.flk");
    cmd.arg("examples/traits.flk");
    cmd.arg("examples/enum.flk");
    let output = cmd.output().expect("failed to execute native flake run");
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");
    let combined = format!("{stdout}{stderr}");
    assert!(output.status.success(), "native check failed: {combined}");
    assert!(combined.contains("ok: examples/hello.flk"));
    assert!(combined.contains("ok: examples/effects.flk"));
    assert!(combined.contains("ok: examples/ownership.flk"));
    assert!(combined.contains("ok: examples/borrow.flk"));
    assert!(combined.contains("ok: examples/nursery.flk"));
    assert!(combined.contains("ok: examples/traits.flk"));
    assert!(combined.contains("ok: examples/enum.flk"));
}

#[test]
fn selfhost_native_binary_check_matches_interpreter() {
    let bin = std::env::temp_dir().join(format!("flake-check-selfhost-{}.exe", std::process::id()));
    let build = flake_bin()
        .current_dir(repo_root())
        .arg("build")
        .arg(selfhost_main())
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("flake build selfhost");
    assert!(
        build.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    struct Remove(std::path::PathBuf);
    impl Drop for Remove {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _bin = Remove(bin.clone());

    let run_bin = |args: &[&str]| {
        let output = Command::new(&bin)
            .current_dir(repo_root())
            .args(args)
            .output()
            .expect("run flake-check-selfhost");
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
    };

    let interp_hello = run_selfhost(
        &["--check", "examples/hello.flk", "examples/traits.flk"],
        false,
    );
    assert!(
        interp_hello.0,
        "interpreter check failed: {}",
        interp_hello.1
    );
    let native_hello = run_bin(&["--check", "examples/hello.flk", "examples/traits.flk"]);
    assert!(
        native_hello.contains("ok: examples/hello.flk"),
        "{native_hello}"
    );
    assert!(
        native_hello.contains("ok: examples/traits.flk"),
        "{native_hello}"
    );
    assert_eq!(
        interp_hello
            .1
            .lines()
            .filter(|l| l.starts_with("ok:"))
            .collect::<Vec<_>>(),
        native_hello
            .lines()
            .filter(|l| l.starts_with("ok:"))
            .collect::<Vec<_>>(),
        "interpreter vs native binary accept mismatch"
    );

    let walk = run_bin(&["--walk", "examples"]);
    assert!(
        walk.contains("Scanned 61 files: all parsed successfully"),
        "native binary walk: {walk}"
    );
}

#[test]
fn selfhost_check_effects_and_ownership_rejections() {
    let dir = std::env::temp_dir().join(format!("flake_selfhost_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);

    // 1. Hidden IO in pure function
    let bad_io = dir.join("bad_io.flk");
    std::fs::write(
        &bad_io,
        "fn shout(s: String) / pure { print(s) }\nfn main() { shout(\"hi\") }",
    )
    .unwrap();

    // 2. Use after move in strict function
    let bad_move = dir.join("bad_move.flk");
    std::fs::write(&bad_move, "strict fn consume(s: String) {}\nstrict fn test(s: String) { consume(s); consume(s) }\nfn main() {}").unwrap();

    // 3. Escaping local reference
    let bad_ref = dir.join("bad_ref.flk");
    std::fs::write(
        &bad_ref,
        "strict fn leak() { let x = 42; return &x }\nfn main() {}",
    )
    .unwrap();

    // 4. Capture ref across spawn
    let bad_spawn = dir.join("bad_spawn.flk");
    std::fs::write(
        &bad_spawn,
        "fn worker(r: &Int) / conc {}\nfn main() / conc { let x = 42; spawn worker(&x) }",
    )
    .unwrap();

    for vm in [false, true] {
        let (_ok_io, out_io) = run_selfhost(&["--check", bad_io.to_str().unwrap()], vm);
        assert!(
            out_io.contains("not declared in `pure`"),
            "expected hidden IO error on vm={vm}: {out_io}"
        );

        let (_ok_mv, out_mv) = run_selfhost(&["--check", bad_move.to_str().unwrap()], vm);
        assert!(
            out_mv.contains("use of moved value"),
            "expected use after move error on vm={vm}: {out_mv}"
        );

        let (_ok_rf, out_rf) = run_selfhost(&["--check", bad_ref.to_str().unwrap()], vm);
        assert!(
            out_rf.contains("cannot return reference to local variable"),
            "expected ref escape error on vm={vm}: {out_rf}"
        );

        let (_ok_sp, out_sp) = run_selfhost(&["--check", bad_spawn.to_str().unwrap()], vm);
        assert!(
            out_sp.contains("cannot capture reference across task boundary into `spawn`"),
            "expected spawn ref error on vm={vm}: {out_sp}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn selfhost_check_multi_file_and_projects() {
    for vm in [false, true] {
        // 1. Multi-file check in a single invocation
        let (ok1, out1) = run_selfhost(
            &[
                "--check",
                "examples/hello.flk",
                "examples/traits.flk",
                "examples/enum.flk",
            ],
            vm,
        );
        assert!(ok1, "failed multi-file check on vm={vm}: {out1}");
        assert!(out1.contains("ok: examples/hello.flk"));
        assert!(out1.contains("ok: examples/traits.flk"));
        assert!(out1.contains("ok: examples/enum.flk"));

        // 2. Multi-file project with dotted submodule imports
        let (ok2, out2) = run_selfhost(&["--check", "examples/projects/v09_flk_scan/main.flk"], vm);
        assert!(ok2, "failed project check on vm={vm}: {out2}");
        assert!(out2.contains("ok: examples/projects/v09_flk_scan/main.flk"));

        // 3. Visibility and sibling module import
        let (ok3, out3) = run_selfhost(&["--check", "examples/visible.flk"], vm);
        assert!(ok3, "failed visible check on vm={vm}: {out3}");
        assert!(out3.contains("ok: examples/visible.flk"));
    }

    // 4. Native multi-file project check
    let mut cmd = flake_bin();
    cmd.current_dir(repo_root());
    cmd.arg("run");
    cmd.arg("--native");
    cmd.arg(selfhost_main());
    cmd.arg("--");
    cmd.arg("--check");
    cmd.arg("examples/projects/v09_flk_scan/main.flk");
    cmd.arg("examples/visible.flk");
    let output = cmd.output().expect("failed to execute native flake run");
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");
    let combined = format!("{stdout}{stderr}");
    assert!(
        output.status.success(),
        "native project check failed: {combined}"
    );
    assert!(combined.contains("ok: examples/projects/v09_flk_scan/main.flk"));
    assert!(combined.contains("ok: examples/visible.flk"));
}

#[test]
fn selfhost_walk_reports_check_errors() {
    let dir = std::env::temp_dir().join(format!("flake_walk_err_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);

    std::fs::write(dir.join("good.flk"), "fn main() {}").unwrap();
    std::fs::write(dir.join("bad.flk"), "fn main() { let x: Int = \"str\" }").unwrap();

    for vm in [false, true] {
        let (_ok, out) = run_selfhost(&["--walk", dir.to_str().unwrap()], vm);
        assert!(
            out.contains("FAIL:"),
            "expected walk failure on vm={vm}: {out}"
        );
        assert!(
            out.contains("1 passed, 1 failed"),
            "expected 1 passed, 1 failed on vm={vm}: {out}"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn selfhost_golden_corpus_agreement() {
    let accept_corpus = [
        "examples/hello.flk",
        "examples/enum.flk",
        "examples/traits.flk",
        "examples/effects.flk",
        "examples/ownership.flk",
        "examples/borrow.flk",
        "examples/modules.flk",
        "examples/projects/v09_flk_scan/main.flk",
        "examples/visible.flk",
        "examples/nursery.flk",
        "examples/concurrency.flk",
        "examples/math.flk",
        "examples/lists.flk",
        "examples/fizzbuzz.flk",
        "examples/fibonacci.flk",
    ];

    for file in &accept_corpus {
        // 1. Rust host checker accepts
        let mut host = flake_bin();
        host.current_dir(repo_root());
        host.arg("check").arg(file);
        let host_out = host.output().expect("run flake check");
        assert!(
            host_out.status.success(),
            "host check failed on {file}: {}",
            String::from_utf8_lossy(&host_out.stderr)
        );

        // 2. Selfhost checker on Interpreter and VM accepts
        for vm in [false, true] {
            let (ok, out) = run_selfhost(&["--check", file], vm);
            assert!(
                ok && out.contains(&format!("ok: {file}")),
                "selfhost check failed on {file} (vm={vm}): {out}"
            );
        }
    }

    // 3. Native selfhost checker accepts the whole corpus
    let mut cmd = flake_bin();
    cmd.current_dir(repo_root());
    cmd.arg("run")
        .arg("--native")
        .arg(selfhost_main())
        .arg("--")
        .arg("--check");
    for file in &accept_corpus {
        cmd.arg(file);
    }
    let native_out = cmd.output().expect("run native check");
    let native_str = String::from_utf8_lossy(&native_out.stdout);
    assert!(
        native_out.status.success(),
        "native selfhost check failed on corpus"
    );
    for file in &accept_corpus {
        assert!(
            native_str.contains(&format!("ok: {file}")),
            "native selfhost missing ok for {file}"
        );
    }
}

#[test]
fn selfhost_golden_reject_corpus_agreement() {
    let dir = std::env::temp_dir().join(format!("flake_reject_corpus_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);

    // Defined negative corpus: comparing Rust host flake check and selfhost --check
    let cases = [
        (
            "bad_effects.flk",
            "fn shout() / pure { print(\"hi\") }\nfn main() { shout() }",
        ),
        (
            "bad_move.flk",
            "strict fn consume(s: String) {}\nstrict fn test(s: String) { consume(s); consume(s) }\nfn main() {}",
        ),
        (
            "bad_escape.flk",
            "strict fn leak() { let x = 42; return &x }\nfn main() {}",
        ),
        (
            "bad_spawn_ref.flk",
            "fn worker(r: &Int) / conc {}\nfn main() / conc { let x = 42; spawn worker(&x) }",
        ),
        ("bad_type.flk", "fn main() { let x: Int = \"hello\" }"),
        ("bad_name.flk", "fn main() { unknown_var_12345() }"),
        (
            "bad_bound.flk",
            "trait Describable { fn describe(self) -> String }\nfn show[T](x: T) { x.describe() }\nfn main() {}",
        ),
    ];

    for (filename, source) in &cases {
        let path = dir.join(filename);
        std::fs::write(&path, source).unwrap();

        // 1. Rust host checker rejects
        let mut host = flake_bin();
        host.current_dir(repo_root());
        host.arg("check").arg(&path);
        let host_out = host.output().expect("run flake check");
        assert!(
            !host_out.status.success(),
            "host check unexpectedly accepted {filename}"
        );

        // 2. Selfhost checker rejects on Interpreter and VM
        for vm in [false, true] {
            let (_ok, out) = run_selfhost(&["--check", path.to_str().unwrap()], vm);
            assert!(
                out.contains("error:"),
                "selfhost check unexpectedly accepted {filename} on vm={vm}: {out}"
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}
