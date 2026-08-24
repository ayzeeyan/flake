# Flake v0.7.0 Release Notes

Theme: **From solid foundation to serious systems language**

Flake v0.7 is a major leap forward, expanding the compiler into a true multi-target native systems language while keeping the pipeline 100% pure Rust with a fully owned backend.

## Highlights

### 1. Multi-Target Native Code Generation
- **Target Configuration & CLI Cross-Targeting**:
  - Target system (`Target`, `TargetArch`, `TargetOs`) supporting Windows PE32+ and Linux ELF64.
  - Added `--target <triple>` option to `flake build` supporting `x86_64-windows`, `x86_64-linux`, and `aarch64-linux`.
- **Pure-Rust ELF64 Generator**:
  - Built custom standalone 64-bit ELF binary encoder (`flake-codegen/src/elf.rs`) with `PT_LOAD` segments and RIP-relative string fixups.
- **Pure-Rust AArch64 Machine Code Assembler**:
  - Built dedicated AArch64 instruction encoder (`flake-codegen/src/aarch64.rs`) supporting ALU, loads/stores, branch/call, and `svc #0` syscalls.

### 2. Concurrency Runtime Foundations
- **Task Lifecycle & State Machine**:
  - Explicit states: `Pending`, `Running`, `Completed(Value)`, `Joined`, `Cancelled`.
- **Inspection Built-ins**:
  - `is_completed(task) -> Bool` returns whether a task has finished.
  - `task_status(task) -> String` returns `"pending"`, `"running"`, `"completed"`, `"joined"`, or `"cancelled"`.
- **Cross-Task Sendability Validation**:
  - Compile-time checking prohibiting borrowed references (`&x`, `&mut x`, `ref T`) from escaping across `spawn` task boundaries.

### 3. Advanced Ownership & Lifetime Analysis
- **Match Arm Pattern Binding Ownership**:
  - Tracked ownership and movement of bindings in pattern matching arms.
- **Structural & Field-Sensitive Borrow Checking**:
  - Field-sensitive and path-based tracking preventing conflicting mutations and invalidations of root and nested containers.
- **Scope & Lifetime Verification**:
  - Checked that reborrows and references do not outlive their owning scope.

### 4. Serious Compiler Optimizations & Package Maturity
- **Algebraic Identity Simplification & Strength Reduction**:
  - Constant folding and algebraic identities on arithmetic, booleans, strings, and reflexive equality.
- **Dead Code Elimination & Jump Threading**:
  - Unreachable block pruning, unused instruction elimination, and jump forwarding.
- **Package Manifest & Lockfile Determinism**:
  - Reliable multi-module resolution and workspace lockfile validation.

### 5. Showcase Projects & Verification
- **Flagship v0.7 Showcase Package**:
  - Comprehensive end-to-end showcase in `examples/projects/v07_showcase/`.
- **100% Cross-Backend Consistency**:
  - All tests passing identically across Interpreter, VM, and Native x86-64 executable backend.
