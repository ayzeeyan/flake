# Flake v0.6.0 Release Notes

Theme: **Make Flake feel like a real systems language**

Flake v0.6.0 represents a major milestone in language maturity, focusing on structured concurrency maturity with nurseries and cancellation, deterministic package lockfiles, rigorous ownership checking with structural borrow conflict detection, and advanced compiler optimizations across our pure-Rust pipeline.

## Highlights

### 1. Structured Concurrency Nurseries & Cancellation Primitives
- **Lexical Task Nurseries (`nursery { ... }`)**:
  - Added lexical `nursery { ... }` block expression to grammar, AST, lexer, parser, type checker, interpreter, VM (`Op::EnterNursery`/`Op::ExitNursery`), and IR lowering.
  - Tasks spawned inside a nursery are registered to the nursery's scope and automatically awaited upon normal block completion in spawn order.
  - Escape analysis prevents task handles from leaking outside the nursery (`contains_task`).
  - Sibling cancellation: child errors or exceptions cancel remaining tasks in the nursery immediately.
- **Task Cancellation Built-ins**:
  - Added `cancel(task: Task[T]) -> Nil` (requires `conc` effect) and `is_cancelled(task: Task[T]) -> Bool`.
  - Consistent error handling across all three execution engines: awaiting a cancelled task produces `"task was cancelled"` runtime error across Interpreter, Bytecode VM, and pure-Rust Native x86-64 executable backend.

### 2. Deterministic Package Lockfiles (`flake.lock`) & CLI Subcommands
- **Lockfile Format & Generation**:
  - Pinned package names, sources, versions, dependency trees, and source content hashes computed via deterministic pure-Rust FNV-1a checksumming.
  - TOML representation with `lockfile_version = 1`, `root_package`, and sorted `[[package]]` entries.
- **CLI Commands**:
  - `flake lock [--check]`: Generates or verifies `flake.lock` for the current package or workspace.
  - `flake update`: Re-resolves package dependency graphs and refreshes `flake.lock`.
  - Automatic lockfile verification during `flake run`, `flake build`, and `flake check`.

### 3. Stronger Ownership & Structural Borrow Checking
- **Structural Borrow Conflict Detection**:
  - Root variable resolution for field accesses and index paths (`root_variable_info`).
  - Enforces move prevention on containing aggregates or containers while fields or elements are borrowed (`cannot move \`p\` while it is borrowed`).
  - Exclusive mutable borrowing checks protecting against conflicting concurrent or mutable sub-path borrows.
- **Branch-Aware Ownership Propagation**:
  - Snapshot isolation and branch state merging across all match arms in `match` expressions.
  - Multi-span move tracking ensuring variables moved in all branches are recognized as moved afterwards.

### 4. Advanced Optimizations & Native Quality
- **Constructor Projection Constant Propagation**:
  - Tracks struct and list constructors (`MakeStruct`, `MakeList`) in `flake-ir` and directly folds subsequent `GetField` and `GetIndex` into constant loads or register moves when unescaped.
  - Extends dead code elimination (`eliminate_dead_instructions`) across pure struct/list constructors and projections when results are unused.
- **CFG Jump Threading & Block Merging**:
  - Added `thread_jumps` optimization pass resolving chained jump targets through intermediate empty basic blocks and converting identical-target branches into direct jumps.
- **Native x86-64 Code Density Optimizations**:
  - Optimized immediate moves (`mov_ri`) to emit 32-bit zero-extending movs for positive 32-bit immediate values, saving 4-5 bytes per immediate load.

### 5. Integration, Showcase & Hardening
- Added [`examples/nursery.flk`](../examples/nursery.flk) showcasing lexical nurseries, parallel task spawns, and explicit task cancellation state querying.
- Full-matrix integration testing across all backends.

## Verification
- Pure Rust pipeline end-to-end (no LLVM, no Cranelift, no C transpilation).
- All workspace tests and clippy checks pass cleanly (`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`).
