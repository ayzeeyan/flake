# Flake v0.13.0 Release Notes: Native Completeness and CTFE Lite

Flake v0.13.0 represents Phase 4 of 6 on the roadmap toward v1.0. It delivers native systems API completeness across Windows and Linux, standalone native compilation of the self-hosted frontend and checker binary, compile-time function evaluation (CTFE lite) with constant folding, and full target matrix support.

Workspace version: **0.13.0**.

---

## 1. Native Systems API Completeness

- **Linux Syscall Runtime (`flake-codegen/src/runtime_linux.rs`)**:
  - Implements direct x86-64 Linux system calls (`sys_mmap`, `sys_open`, `sys_read`, `sys_write`, `sys_close`, `sys_getdents64`, `sys_fork`, `sys_execve`, `sys_wait4`, `sys_getcwd`, `sys_unlink`, `sys_exit`).
  - Emits standalone ELF64 binaries without dynamic linker dependencies, libc requirements, or Win32 imports.
- **Windows PE Systems Support**:
  - Native support for process execution (`process.run`), environment variables (`env`), arguments (`process.program_args` / `args()`), current working directory (`process.current_dir` / `cwd()`), and file/directory operations.
- **AArch64 Linux ELF (Partial Target)**:
  - Generates valid 64-bit ELF binaries (`EM_AARCH64 = 183`) using pure Rust instruction encoding.
  - Core instruction set implemented; systems tooling APIs provided as explicit stubs.
  - Automated tests skip cleanly when no ARM64 or Linux runner is available.
- **Showcase Example**: `examples/systems_native.flk`.

---

## 2. Standalone Native Self-Hosted Frontend & Checker Binary

- **Native Binary Generation**:
  - `flake build selfhost/frontend/main.flk -o flake-check-selfhost.exe` builds a native binary that type-checks Flake programs without the host Rust toolchain.
- **Stable Dynamic List Memory Representation**:
  - Native lists utilize a stable heap header `{len, cap, data*}` preserving pointer integrity and aliased slices across heap reallocations (resolving recursive directory walking in `fs.walk`).
- **Full Repository Verification**:
  - The native selfhost binary verifies all 62 example programs in the repository and parses the entire `selfhost/frontend/` tree.

---

## 3. CTFE Lite: Compile-Time Constant Evaluation

- **Constant Items (`const NAME: T = <expr>`)**:
  - Evaluated and folded during type checking and IR generation.
  - Identical folded values materialized across Tree-walking Interpreter, Bytecode VM, and Native machine code.
- **Pure Constant Functions (`const fn`)**:
  - Pure functions that can be called and evaluated at compile time.
  - Supports function composition, arithmetic, comparisons, boolean logic, conditionals (`if`/`else`), string concatenation, and string interpolation.
- **Sandboxing and Purity Verification**:
  - Compile-time I/O, process execution, concurrency primitives, and statements within const contexts are strictly rejected with precise source spans.
- **Safety Fuel & Recursion Bounds**:
  - `CTFE_FUEL = 10_000` evaluation steps.
  - `MAX_CALL_DEPTH = 256` frames.
  - Prevents compiler hangs, infinite loops, and stack overflows on all host platforms.
- **Showcase Example**: `examples/const_fold.flk`.

---

## 4. Self-Hosted Const Checking Parity

- The self-hosted lexer, parser, and checker (`selfhost/frontend/`) understand `const` declarations and `const fn`.
- The self-hosted checker validates constant expression purity and rejects runtime operations or non-const function calls in constant contexts.
- Automated agreement tests verify identical accept and reject behavior between the host Rust compiler (`flake check`) and the self-hosted checker (`selfhost/frontend/main.flk --check`).

---

## 5. Target Matrix Coverage

- Multi-target cross-compilation verified for:
  - `x86_64-windows`: Windows PE32+
  - `x86_64-linux`: Linux ELF64
  - `aarch64-linux`: AArch64 Linux ELF64 (partial target)
- Complete coverage across `fs`, `process`, `const`, and `selfhost binary`.

---

## Road to v1.0

1. **v0.10.0**: Trait methods and usable bounds (done)
2. **v0.11.0**: Self-hosted frontend (lexer + parser in Flake) (done)
3. **v0.12.0**: Self-hosted checker (types, effects, ownership) (done)
4. **v0.13.0**: Native completeness + CTFE lite (done)
5. **v0.14.0**: Bootstrap (self-hosted compiler producing executables) (next)
6. **v1.0.0**: Freeze and ship
