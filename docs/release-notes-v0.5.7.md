# Flake v0.5.7 Release Notes

Theme: **Deeper packages, more mature concurrency, and further performance work**

Flake v0.5.7 expands package ergonomics with public re-exports and multi-package workspaces, matures structured concurrency ergonomics and type safety, implements module-level IR function inlining, and optimizes native x86-64 machine code emission.

## Highlights

### 1. Package System Expansion & Public Re-exports (`pub import`)
- **Public Re-Exports**:
  - Added `pub import path [as alias]` syntax to grammar, parser, and type checker.
  - Package entrypoints and facade modules can re-export items and submodules (`pub import service`).
  - Downstream consumers can access re-exported functions, structs, and enums seamlessly through the package namespace (`pkg.item`).
- **Workspace & Manifest Improvements (`flake.toml`)**:
  - Added multi-package `[workspace]` manifest support (`members = ["core_api", "hub_app"]`).
  - Added dependency package and version aliasing (`dep = { path = "...", package = "...", version = "..." }`).
  - Strengthened manifest syntax validation with line numbers and actionable diagnostic messages.

### 2. Concurrency Maturity & Ownership Interactions
- **Safe Task Passing**:
  - `Task[T]` handles can be passed across intra-function boundaries (`fn join_both(t1: Task[Int], t2: Task[Int]) -> Int / conc`).
  - Escape analysis rigorously prevents task handles from leaking out of the spawning function scope (`contains_task`).
- **Ownership Move Integration**:
  - Passing `strict owned` values into `spawn work(owned_val)` enforces move semantics at the call site.
- **Cross-Backend Parity**:
  - Consistent execution of tasks returning algebraic data types (`Result[T, E]`), nested patterns, and single-await enforcement across Interpreter, Bytecode VM, and Native x86-64 backend.

### 3. IR & Native Backend Optimizations
- **Module-Level Function Inlining (`flake_ir::opt::inline_functions`)**:
  - Inlining of small, single-block, non-recursive leaf functions directly into call sites.
  - Local variable remapping preserving single-assignment form and operand types.
  - Unlocks subsequent constant folding, copy propagation, and dead instruction elimination passes across inlined call boundaries.
- **Native x86-64 Code Emission Improvements**:
  - Optimized zero-immediate register loading (`mov_ri` with `imm == 0` generates compact `xor reg, reg`).

### 4. Showcase Projects & Integration
- Added [`projects/service_hub`](../examples/projects/service_hub) multi-package workspace demonstrating public re-exports, concurrent worker tasks, algebraic data types, and stdlib integration.
- 100% test coverage and parity across Interpreter, VM, and Native x86-64 targets.

## Verification
- Clean pure-Rust pipeline (no LLVM, no Cranelift, no C transpilation).
- All workspace tests and clippy checks pass cleanly (`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`).
