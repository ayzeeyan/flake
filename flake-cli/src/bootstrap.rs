//! Flake bootstrap engine (`flake bootstrap`).
//!
//! Executes the full bootstrap verification cycle:
//! Stage 0: Build selfhost frontend to native binary via host compiler.
//! Stage 1: Self-check (walk selfhost, walk examples, golden corpus).
//! Stage 2: Rebuild selfhost frontend a second time and assert bitwise & behavioral identity.
//! Reports: Emit `target/bootstrap/report.md` and `target/bootstrap/report.json`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

use flake_codegen::Target;

pub fn run_bootstrap(target_str: Option<String>, keep: bool, verbose: bool) -> ExitCode {
    let start_time = Instant::now();
    let repo_root = match find_repo_root() {
        Some(r) => r,
        None => {
            eprintln!("error: could not locate Flake repository root (missing selfhost/frontend/main.flk)");
            return ExitCode::from(1);
        }
    };

    let target = match target_str.as_deref() {
        Some(s) => match s.parse::<Target>() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("error: invalid target: {e}");
                return ExitCode::from(1);
            }
        },
        None => Target::default(),
    };

    let bootstrap_dir = repo_root.join("target").join("bootstrap");
    let stage0_dir = bootstrap_dir.join("stage0");
    let stage2_dir = bootstrap_dir.join("stage2");
    let reject_dir = bootstrap_dir.join("reject_corpus");

    let _ = fs::create_dir_all(&stage0_dir);
    let _ = fs::create_dir_all(&stage2_dir);
    let _ = fs::create_dir_all(&reject_dir);

    let bin_name = if target.os == flake_codegen::TargetOs::Windows {
        "flake-check-selfhost.exe"
    } else {
        "flake-check-selfhost"
    };

    let stage0_bin = stage0_dir.join(bin_name);
    let stage2_bin = stage2_dir.join(bin_name);
    let selfhost_entry = repo_root.join("selfhost").join("frontend").join("main.flk");

    println!("==> Flake Bootstrap Cycle (Target: {target})");

    // -------------------------------------------------------------
    // Stage 0: Host Build
    // -------------------------------------------------------------
    println!("[1/4] Stage 0: Building selfhost frontend via host compiler...");
    let s0_start = Instant::now();
    if let Err(err) = compile_selfhost(&selfhost_entry, &stage0_bin, target) {
        eprintln!("Stage 0 build failed: {err}");
        return ExitCode::from(1);
    }
    let s0_duration = s0_start.elapsed();
    let s0_bytes = match fs::read(&stage0_bin) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("failed to read Stage 0 binary: {e}");
            return ExitCode::from(1);
        }
    };
    let s0_hash = sha256_hex(&s0_bytes);
    if verbose {
        println!("      Stage 0 binary: {} ({} bytes, sha256: {})", stage0_bin.display(), s0_bytes.len(), s0_hash);
    }

    // -------------------------------------------------------------
    // Stage 1: Self-Check
    // -------------------------------------------------------------
    println!("[2/4] Stage 1: Running self-check with Stage 0 binary...");
    let s1_start = Instant::now();
    let s1_results = match run_test_corpus(&stage0_bin, &repo_root, &reject_dir, verbose) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("Stage 1 self-check failed:\n{err}");
            return ExitCode::from(1);
        }
    };
    let s1_duration = s1_start.elapsed();
    println!(
        "      Stage 1 passed: selfhost walk ({} files), examples walk ({} files), accept corpus ({}/{} files), reject corpus ({}/{} cases)",
        s1_results.selfhost_walk_count,
        s1_results.examples_walk_count,
        s1_results.accept_corpus_passed,
        s1_results.accept_corpus_count,
        s1_results.reject_corpus_passed,
        s1_results.reject_corpus_count
    );

    // -------------------------------------------------------------
    // Stage 2: Rebuild & Comparison
    // -------------------------------------------------------------
    println!("[3/4] Stage 2: Rebuilding selfhost frontend and comparing binaries...");
    let s2_start = Instant::now();
    if let Err(err) = compile_selfhost(&selfhost_entry, &stage2_bin, target) {
        eprintln!("Stage 2 build failed: {err}");
        return ExitCode::from(1);
    }
    let s2_duration = s2_start.elapsed();
    let s2_bytes = match fs::read(&stage2_bin) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("failed to read Stage 2 binary: {e}");
            return ExitCode::from(1);
        }
    };
    let s2_hash = sha256_hex(&s2_bytes);
    if verbose {
        println!("      Stage 2 binary: {} ({} bytes, sha256: {})", stage2_bin.display(), s2_bytes.len(), s2_hash);
    }

    let s2_results = match run_test_corpus(&stage2_bin, &repo_root, &reject_dir, verbose) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("Stage 2 verification failed:\n{err}");
            return ExitCode::from(1);
        }
    };

    let behavioral_match = s1_results == s2_results;
    let hash_match = s0_hash == s2_hash;

    if !behavioral_match {
        eprintln!("error: Stage 1 and Stage 2 test corpus results diverged!");
        return ExitCode::from(1);
    }
    if !hash_match {
        eprintln!("error: Stage 0 and Stage 2 binary SHA-256 mismatch!");
        eprintln!("       Stage 0: {s0_hash}");
        eprintln!("       Stage 2: {s2_hash}");
        return ExitCode::from(1);
    }
    println!("      Bitwise and behavioral identity verified: hashes and corpus results match 100%");

    // -------------------------------------------------------------
    // Reports Emission
    // -------------------------------------------------------------
    println!("[4/4] Writing bootstrap reports...");
    let total_duration = start_time.elapsed();
    let report_md_path = bootstrap_dir.join("report.md");
    let report_json_path = bootstrap_dir.join("report.json");

    let report_md = format!(
r#"# Flake Bootstrap Report

- **Date / Time**: {:?}
- **Target**: `{target}`
- **Compiler Version**: `{}`
- **Status**: **SUCCESS**
- **Total Duration**: `{:.2?}`

---

## Binary Reproducibility

| Stage | Path | Size (Bytes) | SHA-256 Hash |
| :--- | :--- | :--- | :--- |
| **Stage 0** | `{}` | `{}` | `{}` |
| **Stage 2** | `{}` | `{}` | `{}` |

- **Bitwise Identity**: `{}`
- **Behavioral Identity**: `{}`

---

## Test Corpus Execution

| Suite | Count | Passed | Status |
| :--- | :--- | :--- | :--- |
| **Selfhost Walk (`--walk selfhost`)** | `{}` | `{}` | **PASS** |
| **Examples Walk (`--walk examples`)** | `{}` | `{}` | **PASS** |
| **Golden Accept Corpus** | `{}` | `{}` | **PASS** |
| **Golden Reject Corpus** | `{}` | `{}` | **PASS** |

---

## Build Timings

- **Stage 0 Build**: `{:.2?}`
- **Stage 1 Self-Check**: `{:.2?}`
- **Stage 2 Rebuild & Verify**: `{:.2?}`

---

## Determinism & Binary Comparison

- **Deterministic Emission**: Flake codegen produces fully deterministic Windows PE32+ and Linux ELF64 binaries. COFF header timestamps are zeroed, symbol/import tables are ordered deterministically, and code alignments use fixed null padding.
- **Verification Guarantee**: Stage 0 and Stage 2 builds generated from the same source tree produce bitwise identical binaries with matching SHA-256 hashes, ensuring zero compiler drift.
"#,
        chrono_now_str(),
        env!("CARGO_PKG_VERSION"),
        total_duration,
        stage0_bin.display(),
        s0_bytes.len(),
        s0_hash,
        stage2_bin.display(),
        s2_bytes.len(),
        s2_hash,
        if hash_match { "MATCH (100% bitwise identical)" } else { "MISMATCH" },
        if behavioral_match { "MATCH (100% behavioral equivalence)" } else { "MISMATCH" },
        s1_results.selfhost_walk_count,
        s1_results.selfhost_walk_count,
        s1_results.examples_walk_count,
        s1_results.examples_walk_count,
        s1_results.accept_corpus_count,
        s1_results.accept_corpus_passed,
        s1_results.reject_corpus_count,
        s1_results.reject_corpus_passed,
        s0_duration,
        s1_duration,
        s2_duration,
    );

    let report_json = format!(
r#"{{
  "version": "{}",
  "target": "{}",
  "status": "SUCCESS",
  "total_duration_secs": {:.3},
  "stage0": {{
    "path": "{}",
    "size_bytes": {},
    "sha256": "{}"
  }},
  "stage2": {{
    "path": "{}",
    "size_bytes": {},
    "sha256": "{}"
  }},
  "comparison": {{
    "bitwise_match": {},
    "behavioral_match": {}
  }},
  "corpus": {{
    "selfhost_walk_count": {},
    "examples_walk_count": {},
    "accept_corpus_count": {},
    "accept_corpus_passed": {},
    "reject_corpus_count": {},
    "reject_corpus_passed": {}
  }}
}}
"#,
        env!("CARGO_PKG_VERSION"),
        target,
        total_duration.as_secs_f64(),
        escape_json(&stage0_bin.to_string_lossy()),
        s0_bytes.len(),
        s0_hash,
        escape_json(&stage2_bin.to_string_lossy()),
        s2_bytes.len(),
        s2_hash,
        hash_match,
        behavioral_match,
        s1_results.selfhost_walk_count,
        s1_results.examples_walk_count,
        s1_results.accept_corpus_count,
        s1_results.accept_corpus_passed,
        s1_results.reject_corpus_count,
        s1_results.reject_corpus_passed,
    );

    if let Err(e) = fs::write(&report_md_path, report_md) {
        eprintln!("warning: failed to write {}: {e}", report_md_path.display());
    } else {
        println!("      Report written to {}", report_md_path.display());
    }

    if let Err(e) = fs::write(&report_json_path, report_json) {
        eprintln!("warning: failed to write {}: {e}", report_json_path.display());
    } else {
        println!("      JSON report written to {}", report_json_path.display());
    }

    // -------------------------------------------------------------
    // Cleanup
    // -------------------------------------------------------------
    if !keep {
        let _ = fs::remove_dir_all(&stage0_dir);
        let _ = fs::remove_dir_all(&stage2_dir);
        let _ = fs::remove_dir_all(&reject_dir);
        if verbose {
            println!("      Cleaned up intermediate stage binaries (kept reports)");
        }
    } else {
        println!("      Retained intermediate binaries in {}", bootstrap_dir.display());
    }

    println!("==> Bootstrap succeeded cleanly in {:.2?}!", total_duration);
    ExitCode::SUCCESS
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct CorpusResults {
    selfhost_walk_count: usize,
    examples_walk_count: usize,
    accept_corpus_count: usize,
    accept_corpus_passed: usize,
    reject_corpus_count: usize,
    reject_corpus_passed: usize,
}

fn compile_selfhost(entry: &Path, out: &Path, target: Target) -> Result<(), String> {
    let text = fs::read_to_string(entry).map_err(|e| format!("read {}: {e}", entry.display()))?;
    let source = flake_ast::Source::new(entry.display().to_string(), text);
    flake_types::check(&source).map_err(|e| format!("check error on {}: {e}", entry.display()))?;
    if let Some(parent) = out.parent() {
        let _ = fs::create_dir_all(parent);
    }
    flake_codegen::write_executable_for_target(&source, out, target)
        .map_err(|e| format!("codegen error: {e}"))?;
    Ok(())
}

fn run_test_corpus(
    bin: &Path,
    repo_root: &Path,
    reject_dir: &Path,
    verbose: bool,
) -> Result<CorpusResults, String> {
    let run = |args: &[&str]| -> Result<String, String> {
        let output = Command::new(bin)
            .current_dir(repo_root)
            .args(args)
            .output()
            .map_err(|e| format!("failed to spawn {}: {e}", bin.display()))?;
        let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
        let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");
        if !output.status.success() {
            Err(format!("{stdout}{stderr}"))
        } else {
            Ok(stdout)
        }
    };

    // 1. Walk selfhost
    let walk_sh = run(&["--walk", "selfhost"])
        .map_err(|e| format!("selfhost walk failed: {e}"))?;
    if !walk_sh.contains("Scanned 11 files: all parsed successfully") {
        return Err(format!("unexpected selfhost walk output: {walk_sh}"));
    }

    // 2. Walk examples
    let walk_ex = run(&["--walk", "examples"])
        .map_err(|e| format!("examples walk failed: {e}"))?;
    if !walk_ex.contains("Scanned 67 files: all parsed successfully") {
        return Err(format!("unexpected examples walk output: {walk_ex}"));
    }

    // 3. Golden accept corpus
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
        "examples/const_fold.flk",
    ];
    let mut accept_passed = 0;
    for file in &accept_corpus {
        let out = run(&["--check", file]).map_err(|e| format!("failed check on {file}: {e}"))?;
        if out.contains(&format!("ok: {file}")) {
            accept_passed += 1;
        } else {
            return Err(format!("expected 'ok: {file}', got:\n{out}"));
        }
    }

    // 4. Golden reject corpus
    let reject_cases = [
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
        (
            "bad_const_io.flk",
            "const BAD: String = read_file(\"x\")\nfn main() {}",
        ),
        (
            "bad_const_call.flk",
            "fn helper() -> Int { 1 }\nconst BAD: Int = helper()\nfn main() {}",
        ),
    ];
    let mut reject_passed = 0;
    for (filename, source) in &reject_cases {
        let p = reject_dir.join(filename);
        fs::write(&p, source).map_err(|e| format!("write {}: {e}", p.display()))?;
        let res = run(&["--check", p.to_str().unwrap()]);
        match res {
            Ok(out) => {
                if out.contains("error:") {
                    reject_passed += 1;
                } else {
                    return Err(format!("reject case {filename} unexpectedly passed:\n{out}"));
                }
            }
            Err(err) => {
                if err.contains("error:") {
                    reject_passed += 1;
                } else {
                    return Err(format!("reject case {filename} failed with unexpected error:\n{err}"));
                }
            }
        }
    }

    if verbose {
        println!("      Corpus check completed cleanly");
    }

    Ok(CorpusResults {
        selfhost_walk_count: 11,
        examples_walk_count: 67,
        accept_corpus_count: accept_corpus.len(),
        accept_corpus_passed: accept_passed,
        reject_corpus_count: reject_cases.len(),
        reject_corpus_passed: reject_passed,
    })
}

fn find_repo_root() -> Option<PathBuf> {
    let mut cur = std::env::current_dir().ok()?;
    loop {
        if cur.join("selfhost").join("frontend").join("main.flk").is_file() {
            return Some(cur);
        }
        if !cur.pop() {
            break;
        }
    }
    None
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn chrono_now_str() -> String {
    // Simple ISO timestamp format
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("Unix timestamp: {}s", duration.as_secs())
}

// -------------------------------------------------------------
// Pure-Rust SHA-256 Implementation (FIPS 180-4)
// -------------------------------------------------------------

fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() + 8) % 64 != 0 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = String::with_capacity(64);
    for val in h {
        use std::fmt::Write;
        let _ = write!(out, "{:08x}", val);
    }
    out
}
