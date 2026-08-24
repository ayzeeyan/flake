# Flake v0.6.1 Release Notes

Theme: **Stabilize v0.6 — fix bugs, close edge cases, harden reliability**

Flake v0.6.1 is a dedicated stability and bug-fix release addressing edge cases, cross-platform reproducibility, and ownership & optimizer correctness across the pure-Rust compiler pipeline.

## Highlights

### 1. Structured Concurrency & Nursery Hardening
- **Task Handle Containment & Escape Analysis**:
  - Prohibit assigning task handles spawned inside a `nursery { ... }` block to variables declared in an outer scope enclosing the nursery (`cannot assign task handle to variable defined outside the nursery`).
  - Prevent task handles leaking out of nurseries through return expressions or collection wrappers.
- **Robust Early Exit & Nursery Scope Draining**:
  - Automatically cancel active unjoined nursery tasks on early returns, loop breaks, or runtime panics.
  - Full output parity verified across Interpreter, Bytecode VM, and pure-Rust Native x86-64 executable backend.

### 2. Package & Lockfile Cross-Platform Determinism
- **Cross-Platform Deterministic Checksums**:
  - Normalized relative path separators to forward slashes (`/`) during FNV-1a checksum calculation.
  - Normalized CRLF line endings (`\r\n`) to LF (`\n`) for text sources (`.flk`, `flake.toml`), ensuring identical package checksums across Windows, macOS, and Linux.
- **CLI Subcommands**:
  - Hardened drift detection in `flake lock --check` and reliable graph resolution in `flake update`.

### 3. Structural Borrow Checking on Field/Index Mutation
- **Container Borrow Protection**:
  - Prohibit mutating fields or indexing into collections (e.g. `p.x = val` or `arr[i] = val`) while the root container or any of its fields is borrowed (`cannot assign to field of \`p\` while it is borrowed`).
  - Enforced move and ref binding constraints on field assignments.

### 4. Optimizer & Native Backend Correctness
- **Transitive Alias Escape Analysis**:
  - Restrict constructor projection folding (`MakeStruct` -> `GetField`, `MakeList` -> `GetIndex`) to strictly immutable, unmutated locals, preserving dynamic field reads across all aliased mutations.
  - Verified CFG jump threading (`thread_jumps`) and 32-bit zero-extended immediate encoding on Native x86-64.

## Verification
- Pure Rust pipeline end-to-end (no LLVM, no Cranelift, no C transpilation).
- All workspace tests and clippy checks pass cleanly (`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`).
