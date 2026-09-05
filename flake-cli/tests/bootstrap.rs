use std::path::{Path, PathBuf};
use std::process::Command;

fn flake_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_flake"));
    cmd.env("NO_COLOR", "1");
    cmd
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

#[test]
fn bootstrap_command_lifecycle_reports_and_rebuild_verification() {
    let bootstrap_dir = repo_root().join("target").join("bootstrap");

    // 1. Run with --keep and -v
    let mut cmd = flake_bin();
    cmd.current_dir(repo_root());
    cmd.arg("bootstrap").arg("--keep").arg("-v");
    let out = cmd.output().expect("run flake bootstrap --keep -v");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "flake bootstrap --keep -v failed:\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}"
    );

    // 2. Verify --keep preserved Stage 0 and Stage 2 binaries and they are bitwise identical
    let bin_name = if cfg!(windows) {
        "flake-check-selfhost.exe"
    } else {
        "flake-check-selfhost"
    };
    let s0 = bootstrap_dir.join("stage0").join(bin_name);
    let s2 = bootstrap_dir.join("stage2").join(bin_name);

    assert!(s0.is_file(), "expected stage0 binary to exist with --keep at {}", s0.display());
    assert!(s2.is_file(), "expected stage2 binary to exist with --keep at {}", s2.display());

    let b0 = std::fs::read(&s0).expect("read s0");
    let b2 = std::fs::read(&s2).expect("read s2");
    assert_eq!(b0, b2, "stage 0 and stage 2 binaries must be bitwise identical");

    // 3. Verify report.md and report.json exist and contain expected sections
    let report_md = bootstrap_dir.join("report.md");
    let report_json = bootstrap_dir.join("report.json");

    assert!(report_md.is_file(), "expected target/bootstrap/report.md to exist");
    assert!(report_json.is_file(), "expected target/bootstrap/report.json to exist");

    let md_content = std::fs::read_to_string(&report_md).expect("read report.md");
    assert!(md_content.contains("**Status**: **SUCCESS**"), "md status not success");
    assert!(md_content.contains("Bitwise Identity"), "md missing bitwise identity");
    assert!(md_content.contains("Behavioral Identity"), "md missing behavioral identity");
    assert!(md_content.contains("Selfhost Walk (`--walk selfhost`)"), "md missing selfhost walk");
    assert!(md_content.contains("Deterministic Emission"), "md missing determinism documentation");

    let json_content = std::fs::read_to_string(&report_json).expect("read report.json");
    assert!(json_content.contains(r#""status": "SUCCESS""#), "json status not success");
    assert!(json_content.contains(r#""bitwise_match": true"#), "json bitwise_match not true");
    assert!(json_content.contains(r#""behavioral_match": true"#), "json behavioral_match not true");
    assert!(json_content.contains(r#""selfhost_walk_count": 11"#), "json missing selfhost count");
    assert!(json_content.contains(r#""examples_walk_count": 67"#), "json missing examples count");
    assert!(json_content.contains(r#""accept_corpus_passed": 16"#), "json missing accept passed");
    assert!(json_content.contains(r#""reject_corpus_passed": 9"#), "json missing reject passed");

    // 4. Run without --keep to verify default cleanup of stage binaries
    let mut cmd2 = flake_bin();
    cmd2.current_dir(repo_root());
    cmd2.arg("bootstrap");
    let out2 = cmd2.output().expect("run flake bootstrap");
    assert!(out2.status.success(), "flake bootstrap (default cleanup) failed");

    assert!(!s0.exists(), "stage0 binary should be cleaned up by default");
    assert!(!s2.exists(), "stage2 binary should be cleaned up by default");
    assert!(report_md.is_file(), "report.md should remain after cleanup");
    assert!(report_json.is_file(), "report.json should remain after cleanup");
}
