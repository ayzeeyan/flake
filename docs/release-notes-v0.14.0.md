# Flake v0.14.0 Release Notes: Bootstrap

Flake v0.14.0 represents Phase 5 of 6 on the roadmap toward v1.0. It delivers a complete, automated, and reproducible **bootstrap loop** where the self-hosted Flake frontend & semantic checker is compiled natively, verifies its own source tree and the full golden test corpus, rebuilds itself, and asserts 100% bitwise and behavioral identity.

Workspace version: **0.14.0**.

---

## 1. The `flake bootstrap` Command

Flake v0.14 introduces the top-level `flake bootstrap` command in `flake-cli`:
- **Stage 0 (Host Build)**: Compiles `selfhost/frontend/main.flk` into a standalone native executable (`target/bootstrap/stage0/flake-check-selfhost.exe` on Windows, or ELF executable on Linux).
- **Stage 1 (Mandatory Self-Check)**: Executes the Stage 0 native binary across:
  - `--walk selfhost`: Scans and parses all 11 files of the self-hosted compiler frontend.
  - `--walk examples`: Scans and parses all 62 example programs in the repository.
  - Golden accept corpus: Type-checks all 16 golden accept test files.
  - Golden reject corpus: Confirms that all 9 illegal language patterns (effects, moves, escaping references, concurrency boundaries, invalid types, impure const fns) are rejected with accurate source spans.
- **Stage 2 (Rebuild & Verification)**: Rebuilds `selfhost/frontend/main.flk` a second time into an isolated path (`target/bootstrap/stage2/flake-check-selfhost.exe`) and verifies:
  - **Behavioral Identity**: Stage 1 and Stage 2 produce identical accept and reject outputs.
  - **Bitwise Identity**: Both binaries produce matching SHA-256 hashes.
- **Flags**:
  - `--target <triple>`: Target architecture/OS (`x86_64-windows`, `x86_64-linux`, `aarch64-linux`).
  - `--keep`: Retain intermediate stage binaries in `target/bootstrap/`.
  - `-v`, `--verbose`: Detailed timing and diagnostic outputs.

---

## 2. Stable-Subset Lock for `selfhost/`

- Every file in `selfhost/` is audited by an automated AST-level visitor test (`flake-cli/tests/selfhost_subset_lock.rs`).
- Prohibits macro keywords, unapproved effects, host-only constructs, foreign call conventions, and unapproved builtins.
- Guarantees dual-checker agreement: both the host compiler `flake check` and the selfhost checker `--walk selfhost` pass cleanly.

---

## 3. Bitwise Determinism & Reproducibility

- Flake codegen produces 100% deterministic Windows PE32+ and Linux ELF64 binaries:
  - Zeroed COFF header timestamps (`TimeDateStamp = 0`).
  - Deterministically ordered section headers and symbol tables.
  - Fixed null-byte section alignment padding.
- Guarantees zero compiler drift between builds.

---

## 4. Structured Bootstrap Reporting

- Automatically emits:
  - `target/bootstrap/report.md`: Formatted Markdown audit table detailing targets, binary hashes, test counts, timings, and determinism verification.
  - `target/bootstrap/report.json`: Structured JSON document for CI/CD integration and machine verification.

---

## 5. Architectural Boundaries (Anti-Scope)

- The pure Rust host compiler (`flake-parser`, `flake-types`, `flake-ir`, `flake-codegen`, `flake-cli`) remains the authoritative compiler of record.
- Flake does not yet include a self-hosted native machine-code assembler or ELF/PE emitter written in Flake.
- Macro systems, compile-time I/O, and arbitrary compile-time code execution remain explicitly out of scope.

---

## Road to v1.0

1. **v0.10.0**: Trait methods and usable bounds (done)
2. **v0.11.0**: Self-hosted frontend (lexer + parser in Flake) (done)
3. **v0.12.0**: Self-hosted checker (types, effects, ownership) (done)
4. **v0.13.0**: Native completeness + CTFE lite (done)
5. **v0.14.0**: Bootstrap (selfhost checks and rebuilds itself) (done)
6. **v1.0.0**: Freeze and ship (next)
