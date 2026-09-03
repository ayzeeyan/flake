# Flake v1.0 Quality Gate & Verification Checklist

This document defines the **v1.0 release quality gate** and testing hierarchy for Flake. Every release must pass this checklist in its entirety before shipping.

---

## 1. The v1.0 Quality Gate Checklist

The quality gate comprises five mandatory verification tiers executed on the primary native target:

```bash
# Gate 1: Full workspace unit & integration test suite
cargo test --workspace

# Gate 2: Workspace-wide strict linter check
cargo clippy --workspace --all-targets -- -D warnings

# Gate 3: Tri-backend semantic consistency suite (Interpreter = VM = Native)
cargo test -p flake-cli --test backend_consistency

# Gate 4: Dual-checker & frozen stable subset lock (Selfhost + Examples)
cargo test -p flake-cli --test selfhost_subset_lock
cargo test -p flake-cli --test selfhost_frontend

# Gate 5: Automated self-rebuilding bootstrap cycle (Stage 0 -> Stage 1 -> Stage 2)
cargo run -p flake-cli -- bootstrap -v
cargo test -p flake-cli --test bootstrap
```

### Verification Criteria

| Gate | Target Command | Scope | Pass Criteria |
| :--- | :--- | :--- | :--- |
| **Gate 1** | `cargo test --workspace` | All 9 workspace crates | 100% tests passing, 0 failures. |
| **Gate 2** | `cargo clippy --workspace --all-targets -- -D warnings` | All crates, binaries, tests, benchmarks | 0 warnings. |
| **Gate 3** | `backend_consistency.rs` | Interpreter, VM, and Native x86-64 | Identical stdout, return codes, and error markers across all three execution engines. |
| **Gate 4** | `selfhost_subset_lock.rs` | 11 selfhost files + 62 examples | All files adhere to frozen stable subset; host `flake check` and selfhost `--walk` pass. |
| **Gate 5** | `flake bootstrap` | Stage 0, Stage 1, Stage 2 | Stage 1 passes all selfhost and corpus tests; Stage 2 achieves 100% bitwise hash match (`SHA-256`) and behavioral identity. Report generated at `target/bootstrap/report.md`. |

---

## 2. Test Layer Architecture

Flake organizes testing into focused layers:

1. **Frontend Unit Tests** (`flake-lexer`, `flake-parser`, `flake-types`):
   - Tokenization extents, source span mapping, line/col tracking.
   - AST node construction and visitor integrity.
   - Type inference, trait method dispatch, gradual typing (`dyn`), algebraic effects, linear ownership, and borrow checking.
2. **Runtime Unit Tests** (`flake-interpreter`, `flake-vm`):
   - Control flow (`if`, `match`, loops, recursion).
   - Structured task scopes (`spawn`, `await`, `nursery`).
   - Scalar and collection operations (`List`, `Map`, strings, math).
   - Deterministic error propagation and Result-style `?`.
3. **IR and Code Generation Tests** (`flake-ir`, `flake-codegen`):
   - CFG lowering, SSA generation, and liveness analysis.
   - Register allocation and callee-saved register preservation.
   - PE32+ (Windows) and ELF64 (Linux syscall runtime) binary formatting.
   - Multi-target cross-compilation matrix (`x86_64-windows`, `x86_64-linux`, `aarch64-linux`).
4. **Backend Consistency Matrix** (`flake-cli/tests/backend_consistency.rs`):
   - Table-driven programs covering arithmetic, string interpolation, enums, pattern matching, nursery concurrency, channels, and constant folding.
   - Asserts identical observable execution and stdout across the tree-walking interpreter, stack bytecode VM, and native machine code.
5. **Self-Hosted Checker & Bootstrap Tests** (`flake-cli/tests/selfhost_*.rs`, `bootstrap.rs`):
   - Dual-checker parity between host `flake check` and `selfhost/frontend/main.flk`.
   - Mandatory self-check on golden accept (16 files) and reject (9 cases) corpora.
   - Stage 0 vs Stage 2 bitwise rebuild reproducibility.

---

## 3. Consistency Contract

For every valid program within the frozen stable subset:
- **Output Agreement**: Formatted values, printed text, and return values are identical across Interpreter, VM, and Native backends.
- **Determinism**: Map display maintains deterministic typed-key order. Floating-point comparisons and NaN handling agree across platforms.
- **Span Precision**: Diagnostics pinpoint source file, line, and column spans identically across all compiler tiers.
