# Flake v1.0.0 Release Notes: Frozen and Shipped

**Clarity, crystallized.**

Flake v1.0.0 is the official, frozen release of Flake, completing the six-phase roadmap from prototype to production systems language. It establishes the frozen language specification, introduces the 1.x backward compatibility guarantee, and proves compiler integrity through an automated, bitwise-deterministic bootstrap loop.

Workspace version: **1.0.0**.

---

## 1. The v1.0 Backward Compatibility Promise

Flake v1.0.0 introduces a strict stability contract documented in [docs/stable-subset.md](stable-subset.md):
- **1.x Compatibility**: Any program conforming to the frozen stable subset specification that `flake check` accepts in Flake 1.0.0 is guaranteed to remain accepted, compile, and preserve identical observable runtime semantics across all future 1.x releases (`1.1.0`, `1.2.0`, etc.).
- **No Breaking Surface Changes**: Syntax changes, breaking API removals in the standard library, or alterations to ownership and effect semantics will never occur within 1.x. Any breaking revision is reserved exclusively for a future Flake 2.0.
- **Soundness Exceptions**: The only permissible changes are explicit bug fixes to close unintentional soundness or safety holes.

---

## 2. Core Pillars of Flake v1.0

### 1. Pure-Rust Owned Compiler Pipeline
- Entirely self-contained compiler pipeline implemented in pure Rust without LLVM, Cranelift, or C transpilation:
  `source → flake-lexer → flake-parser → flake-types → flake-ir → flake-codegen → native binary (PE32+ / ELF64)`
- Tree-walking interpreter (`flake-interpreter`) and bytecode virtual machine (`flake-vm`) maintain 100% observable execution parity with native machine code.

### 2. Gradual Typing and Parametric Generics
- Statically typed by default with local type inference and an explicit `dyn` escape hatch for gradual interop.
- Parametric polymorphism across functions, structs, algebraic enums, and type aliases.
- Marker and comparison traits (`Eq`, `Ord`, `Hash`) and user-defined traits with method dispatch on concrete types and generic bounds (`fn max[T: Ord](a: T, b: T) -> T`).

### 3. First-Class Algebraic Effect System
- Side effects are explicit in function signatures after `/`:
  - `io`: Terminal, filesystem, process operations.
  - `alloc`: Heap allocations.
  - `conc`: Structured concurrency operations.
  - `panic`: Unrecoverable errors and assertion failures.
  - `pure`: Explicitly side-effect-free computation.
- Automatic effect inference propagates required effects from unannotated helpers to callers.
- Strict effect boundary enforcement prevents pure functions from invoking unannotated or impure routines.

### 4. Gradual Ownership and Safe Borrowing
- Opt-in ownership via `strict fn` and `owned fn` prevents ceremony on everyday scripts while providing linear move checking at critical systems boundaries.
- Shared immutable references `&T` and exclusive mutable references `&mut T`.
- Rejects escaping local stack references (`return &local`).
- Concurrency sendability rules disallow borrowed references across task boundaries.

### 5. Structured Concurrency
- `spawn <call>` launches scope-bound child tasks producing typed `Task[T]` handles under the `conc` effect.
- `await <task>` joins a task and yields its result. Tasks may only be awaited once.
- `nursery { ... }` ensures that all tasks spawned within a lexical scope complete before execution leaves the block.
- Built-in cancellation and task status inspection (`cancel`, `is_completed`, `is_cancelled`, `task_status`).

### 6. CTFE Lite (Compile-Time Function Evaluation)
- Top-level `const NAME: T = <expr>` evaluated and folded during type check and IR lowering.
- Pure `const fn` evaluated at compile time without I/O or concurrency side effects.
- Constant folding for arithmetic, comparisons, boolean logic, `if`/`else` branching, string concatenation, and string interpolation.
- Bounded fuel (`CTFE_FUEL = 10_000`) and recursion depth limits (`MAX_CALL_DEPTH = 256`) guarantee compile-time termination.

### 7. Multi-Target Native Code Generation
- Direct emission of standalone executables:
  - **`x86_64-windows`**: PE32+ binaries with Win32 systems runtime (`KERNEL32.dll`).
  - **`x86_64-linux`**: Standalone ELF64 binaries with direct Linux syscall runtime (`runtime_linux.rs`), requiring no dynamic linker or libc.
  - **`aarch64-linux`**: 64-bit ELF emission with core instruction encodings (partial target; systems APIs are explicit stubs).
- 100% bitwise deterministic emission (zeroed COFF timestamps, fixed ELF headers, and deterministic section order).

### 8. Self-Hosted Frontend & Checker
- Full semantic analysis pipeline written purely in Flake under `selfhost/frontend/`:
  - `tokens.flk`, `lexer.flk`, `ast.flk`, `parser.flk`, `check.flk`, `scope.flk`, `types.flk`, `effects.flk`, `ownership.flk`, `main.flk`.
- Tested and verified against the golden accept and reject test corpora.

### 9. Automated Bootstrap Verification (`flake bootstrap`)
- Complete self-rebuilding bootstrap cycle:
  - **Stage 0**: Host compiles selfhost frontend to native binary.
  - **Stage 1**: Native Stage 0 binary verifies selfhost sources (`--walk selfhost`), examples (`--walk examples`), and the full test corpus.
  - **Stage 2**: Host rebuilds the selfhost binary to an isolated path.
  - **Verification**: Asserts 100% behavioral equivalence and bitwise hash match (`SHA-256`), proving zero compiler drift.
  - Generates `target/bootstrap/report.md` and `target/bootstrap/report.json`.

### 10. Developer Tooling
- `flake run`: Tree-walking interpreter, `--vm` bytecode VM, and `--native` execution.
- `flake build`: Native executable generation with optional `--emit-asm` assembly listings.
- `flake check`: Type, effect, and ownership checking without running.
- `flake ir`: Control-flow graph intermediate representation inspection.
- `flake bootstrap`: End-to-end bootstrap verification.
- `flake repl`: Interactive Read-Eval-Print Loop.
- `flake init` / `flake new`: Package creation.
- `flake lock` / `flake update`: Deterministic package manifests (`flake.toml`) and lockfiles (`flake.lock`).

---

## 3. What Flake v1.0 Is NOT (Anti-Scope)

To ensure focus and long-term stability, the following features are explicitly out of scope for Flake v1.0 and will not be added during the 1.x series:
- **Macro Systems**: No procedural macros, AST quotation, or token-tree expansion.
- **Compile-Time I/O**: CTFE is pure and cannot access disks, networks, or spawn processes.
- **Host Compiler Replacement**: The pure Rust compiler pipeline remains the authoritative compiler of record.
- **Flake-Written Codegen**: The machine code emitter remains implemented in Rust.
- **Alternative Runtimes**: No C transpilation, LLVM, or Cranelift dependencies.

---

## 4. Post-1.0 Roadmap (The 1.x Era)

All post-1.0 releases will follow semantic versioning within the 1.x compatibility promise:
1. **AArch64 Native Systems Runtime**: Completing the systems API runtime for AArch64 Linux to bring it to full Tier 1 parity.
2. **Backend Optimizations**: Register coloring enhancements, peephole optimizations, and dead-code elimination.
3. **Tooling & Language Server**: Richer IDE integration, Language Server Protocol (LSP) diagnostics, and formatting.
4. **Standard Library Additions**: Safe, modular utility libraries adhering to the frozen language specification.

---

## 5. Acknowledgments

Flake v1.0 is the result of systematic engineering across six dedicated roadmap phases:
- **v0.10**: Trait methods and usable bounds
- **v0.11**: Self-hosted lexer and parser in Flake
- **v0.12**: Self-hosted semantic checker (types, effects, ownership)
- **v0.13**: Native systems completeness and CTFE lite
- **v0.14**: Reproducible self-rebuilding bootstrap
- **v1.0.0**: Language freeze, normative specification, and official release
