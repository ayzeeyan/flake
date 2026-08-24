use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQUE: AtomicU64 = AtomicU64::new(0);

fn flake_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_flake"));
    cmd.env("NO_COLOR", "1");
    cmd
}

fn temp_source(label: &str) -> PathBuf {
    let id = UNIQUE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("flake-cli-{label}-{}-{id}.flk", std::process::id()))
}

#[test]
fn version_flag_prints_semver() {
    let output = flake_bin().arg("--version").output().expect("run flake");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("flake"), "stdout: {stdout}");
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "stdout: {stdout}"
    );
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
fn run_rejects_conflicting_backend_flags() {
    let source = temp_source("backend-conflict");
    std::fs::write(&source, "fn main() {}\n").expect("write source");
    let output = flake_bin()
        .arg("run")
        .arg("--vm")
        .arg("--native")
        .arg(&source)
        .output()
        .expect("reject backend conflict");
    let _ = std::fs::remove_file(source);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot be used with"), "{stderr}");
    assert!(
        stderr.contains("--vm") && stderr.contains("--native"),
        "{stderr}"
    );
}

#[test]
fn overloaded_builtin_errors_are_reported_before_execution() {
    let source = temp_source("builtin-arity");
    std::fs::write(&source, "fn main() { range(1, 2, 3) }\n").expect("write source");
    let output = flake_bin()
        .arg("check")
        .arg(&source)
        .output()
        .expect("check builtin arity");
    let _ = std::fs::remove_file(source);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("range() expected 1 or 2"), "{stderr}");
    assert!(stderr.contains("help"), "{stderr}");
}

#[test]
fn vm_runtime_diagnostic_highlights_the_failing_expression() {
    let source = temp_source("vm-runtime-span");
    std::fs::write(
        &source,
        "fn main() {\n    let numerator = 42\n    print(numerator / 0)\n}\n",
    )
    .expect("write source");
    let output = flake_bin()
        .arg("run")
        .arg("--vm")
        .arg(&source)
        .output()
        .expect("run failing VM program");
    let _ = std::fs::remove_file(source);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("division by zero"), "{stderr}");
    assert!(stderr.contains("numerator / 0"), "{stderr}");
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

#[test]
fn build_emits_only_the_executable_by_default() {
    let source = temp_source("build-exe");
    let exe = source.with_extension("exe");
    let asm = source.with_extension("s");
    std::fs::write(&source, "fn main() { print(42) }\n").expect("write source");
    std::fs::write(&exe, b"stale executable").expect("seed old output");

    let output = flake_bin()
        .arg("build")
        .arg(&source)
        .arg("--output")
        .arg(&exe)
        .output()
        .expect("build executable");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(exe.is_file(), "native executable was not written");
    assert!(!asm.exists(), "assembly should be opt-in");
    let native = Command::new(&exe).output().expect("run built executable");
    assert!(native.status.success());
    assert_eq!(String::from_utf8_lossy(&native.stdout), "42\n");

    let _ = std::fs::remove_file(source);
    let _ = std::fs::remove_file(exe);
}

#[test]
fn build_emit_asm_writes_an_explicit_listing() {
    let source = temp_source("build-asm");
    let exe = source.with_extension("exe");
    let asm = source.with_extension("s");
    std::fs::write(&source, "fn main() { print(42) }\n").expect("write source");

    let output = flake_bin()
        .arg("build")
        .arg(&source)
        .arg("--output")
        .arg(&exe)
        .arg("--emit-asm")
        .output()
        .expect("build with assembly");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(exe.is_file());
    let listing = std::fs::read_to_string(&asm).expect("assembly listing");
    assert!(listing.contains("Flake x86-64"), "{listing}");
    assert!(listing.contains("main:"), "{listing}");

    let _ = std::fs::remove_file(source);
    let _ = std::fs::remove_file(exe);
    let _ = std::fs::remove_file(asm);
}

#[test]
fn build_rejects_type_errors_before_writing_artifacts() {
    let source = temp_source("build-check");
    let exe = source.with_extension("exe");
    let asm = source.with_extension("s");
    std::fs::write(
        &source,
        "fn needs_int(value: Int) {}\nfn main() { needs_int(\"not an int\") }\n",
    )
    .expect("write invalid source");

    let output = flake_bin()
        .arg("build")
        .arg(&source)
        .arg("--output")
        .arg(&exe)
        .arg("--emit-asm")
        .output()
        .expect("reject invalid build");
    assert!(!output.status.success());
    assert!(!exe.exists(), "invalid build wrote an executable");
    assert!(!asm.exists(), "invalid build wrote an assembly listing");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("type") || stderr.contains("expected"),
        "{stderr}"
    );

    let _ = std::fs::remove_file(source);
}

#[test]
fn package_new_and_run() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let pkg_dir = std::env::temp_dir().join(format!("flake-pkg-test-{nonce}"));

    // 1. flake new <dir>
    let output = flake_bin()
        .arg("new")
        .arg(&pkg_dir)
        .output()
        .expect("flake new");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(pkg_dir.join("flake.toml").is_file());
    assert!(pkg_dir.join("main.flk").is_file());

    // 2. flake run <dir>
    let run_out = flake_bin()
        .arg("run")
        .arg(&pkg_dir)
        .output()
        .expect("flake run package dir");
    assert!(run_out.status.success(), "stderr: {}", String::from_utf8_lossy(&run_out.stderr));
    let stdout = String::from_utf8_lossy(&run_out.stdout);
    assert!(stdout.contains("Hello from"), "stdout: {stdout}");

    // Cleanup
    let _ = std::fs::remove_dir_all(&pkg_dir);
}

#[test]
fn package_pub_reexport_and_workspace() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("flake-reexport-test-{nonce}"));
    let lib_dir = root.join("core_lib");
    let app_dir = root.join("app");

    std::fs::create_dir_all(&lib_dir).expect("create lib dir");
    std::fs::create_dir_all(&app_dir).expect("create app dir");

    // Write core_lib/service.flk
    std::fs::write(
        lib_dir.join("service.flk"),
        "pub fn calculate(x: Int, y: Int) -> Int { x * 10 + y }\n",
    )
    .expect("write service.flk");

    // Write core_lib/main.flk with pub import
    std::fs::write(
        lib_dir.join("main.flk"),
        "pub import service\npub fn greeting() -> String { \"hello from core\" }\n",
    )
    .expect("write core_lib main.flk");

    // Write core_lib/flake.toml
    std::fs::write(
        lib_dir.join("flake.toml"),
        "[package]\nname = \"core_lib\"\nversion = \"0.1.0\"\nentry = \"main.flk\"\n",
    )
    .expect("write core_lib flake.toml");

    // Write app/flake.toml
    std::fs::write(
        app_dir.join("flake.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nentry = \"main.flk\"\n\n[dependencies]\ncore_lib = { path = \"../core_lib\" }\n",
    )
    .expect("write app flake.toml");

    // Write app/main.flk calling re-exported service.calculate via core_lib
    std::fs::write(
        app_dir.join("main.flk"),
        "import core_lib\nfn main() / io {\n    print(core_lib.greeting())\n    print(core_lib.calculate(4, 2))\n}\n",
    )
    .expect("write app main.flk");

    // 1. Run tree interpreter
    let out_interp = flake_bin().arg("run").arg(&app_dir).output().expect("run interp");
    assert!(out_interp.status.success(), "interp err: {}", String::from_utf8_lossy(&out_interp.stderr));
    assert_eq!(String::from_utf8_lossy(&out_interp.stdout), "hello from core\n42\n");

    // 2. Run bytecode VM
    let out_vm = flake_bin().arg("run").arg("--vm").arg(&app_dir).output().expect("run vm");
    assert!(out_vm.status.success(), "vm err: {}", String::from_utf8_lossy(&out_vm.stderr));
    assert_eq!(String::from_utf8_lossy(&out_vm.stdout), "hello from core\n42\n");

    // 3. Run Native x86-64
    let out_native = flake_bin().arg("run").arg("--native").arg(&app_dir).output().expect("run native");
    assert!(out_native.status.success(), "native err: {}", String::from_utf8_lossy(&out_native.stderr));
    assert_eq!(String::from_utf8_lossy(&out_native.stdout), "hello from core\n42\n");

    // Cleanup
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn package_lock_and_update_commands() {
    let root = std::env::temp_dir().join(format!("flake-test-lock-{}", std::process::id()));
    let lib_dir = root.join("math_lib");
    let app_dir = root.join("app");
    std::fs::create_dir_all(&lib_dir).expect("create lib dir");
    std::fs::create_dir_all(&app_dir).expect("create app dir");

    std::fs::write(
        lib_dir.join("main.flk"),
        "pub fn square(x: Int) -> Int { x * x }\n",
    )
    .expect("write math_lib main.flk");

    std::fs::write(
        lib_dir.join("flake.toml"),
        "[package]\nname = \"math_lib\"\nversion = \"0.1.0\"\nentry = \"main.flk\"\n",
    )
    .expect("write math_lib flake.toml");

    std::fs::write(
        app_dir.join("flake.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nentry = \"main.flk\"\n\n[dependencies]\nmath_lib = { path = \"../math_lib\" }\n",
    )
    .expect("write app flake.toml");

    std::fs::write(
        app_dir.join("main.flk"),
        "import math_lib\nfn main() / io {\n    print(math_lib.square(9))\n}\n",
    )
    .expect("write app main.flk");

    // 1. Run flake lock on app
    let out_lock = flake_bin().arg("lock").arg(&app_dir).output().expect("run lock");
    assert!(out_lock.status.success(), "lock stderr: {}", String::from_utf8_lossy(&out_lock.stderr));
    let lock_file = app_dir.join("flake.lock");
    assert!(lock_file.is_file(), "flake.lock should exist");
    let lock_content = std::fs::read_to_string(&lock_file).expect("read flake.lock");
    assert!(lock_content.contains("lockfile_version = 1"));
    assert!(lock_content.contains("name = \"math_lib\""));
    assert!(lock_content.contains("name = \"app\""));

    // 2. Check lock is up to date
    let out_check = flake_bin().arg("lock").arg("--check").arg(&app_dir).output().expect("check lock");
    assert!(out_check.status.success(), "lock --check failed");

    // 3. Run app with lockfile present
    let out_run = flake_bin().arg("run").arg(&app_dir).output().expect("run app");
    assert!(out_run.status.success());
    assert_eq!(String::from_utf8_lossy(&out_run.stdout), "81\n");

    // 4. Run flake update
    let out_update = flake_bin().arg("update").arg(&app_dir).output().expect("run update");
    assert!(out_update.status.success());
    assert!(lock_file.is_file());

    // 5. Mutate manifest version to simulate drift and test lock --check failure
    std::fs::write(
        app_dir.join("flake.toml"),
        "[package]\nname = \"app\"\nversion = \"0.2.0\"\nentry = \"main.flk\"\n\n[dependencies]\nmath_lib = { path = \"../math_lib\" }\n",
    )
    .expect("write updated app flake.toml");

    let out_check_drift = flake_bin().arg("lock").arg("--check").arg(&app_dir).output().expect("check lock drift");
    assert!(!out_check_drift.status.success(), "lock --check should fail when manifest drifts");
    let err_msg = String::from_utf8_lossy(&out_check_drift.stderr);
    assert!(err_msg.contains("mismatch") || err_msg.contains("run `flake update`"), "err: {err_msg}");

    // Cleanup
    let _ = std::fs::remove_dir_all(&root);
}

