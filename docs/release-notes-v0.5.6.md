# Flake v0.5.6 Release Notes

Theme: **Strengthen concurrency, introduce package foundations, and add real optimizations**

Flake v0.5.6 expands language capabilities, strengthens the concurrency model across all execution targets, introduces local package manifests and multi-package layouts, and implements aggressive IR optimization passes and native backend improvements.

## Highlights

### 1. Strengthened Concurrency (`conc` effect parity)
- **Heap-Allocated Typed Task Representation on Native x86-64**:
  - `Task[T]` objects in native machine code now use structured heap descriptors with state tracking (`Pending`, `Joined`, `Running`, `Cancelled`).
  - Added `IrType::Task`, `Inst::Spawn`, and `Inst::Await` directly to the Flake custom IR.
- **Strict Single-Join Runtime Contract**:
  - All three backends (Tree-walking Interpreter, Bytecode VM, and Native x86-64) enforce identical task lifecycle semantics: attempting to await an already-joined task raises `"task was already awaited"` runtime failure.

### 2. Package Manager Foundations & Manifests (`flake.toml`)
- **Manifest Format (`flake.toml`)**:
  - Declarative package configuration supporting metadata (`name`, `version`, `entry`, `authors`, `description`) and local file-path dependencies (`[dependencies]`).
- **Seamless Local Dependency Resolution**:
  - `flake-parser::resolve` seamlessly discovers parent manifests and resolves package and nested module imports (`import dep.submodule`).
- **CLI Package Commands**:
  - `flake init [--name <name>]`: initializes a package in the current working directory.
  - `flake new <path>`: creates a new package directory with manifest and entrypoint.
  - `flake run`, `flake check`, `flake build`, `flake ir`: automatic discovery and execution of manifest entrypoints or target directories without needing explicit file arguments.

### 3. IR & Native Backend Optimizations
- **Constant Folding & Propagation (`flake-ir::opt`)**:
  - Compile-time evaluation of pure scalar expressions (integers, floats, booleans, string operations) with checked runtime overflow safety.
  - Constant branch simplification turning conditional branches into direct jumps.
  - Cross-block constant propagation for single-assignment locals.
- **Dead Code & Block Elimination**:
  - Reachability analysis from function entries eliminating dead blocks.
  - Pruning unused pure instructions (`LoadConst`, `Unary`, `Binary`, `Move`, `LoadFunction`).
- **Copy & Redundant Move Propagation**:
  - Copy propagation removing unnecessary variable moves.
- **x86-64 Assembler Peephole Optimizations**:
  - Redundant self-move elimination (`mov reg, reg` skipped).

### 4. Integration & Multi-Package Examples
- Added `examples/projects/pkg_workspace` showcasing multi-package local architectures with inter-package dependencies and nested submodule consumption.
- Validated 100% cross-backend consistency across Interpreter, Bytecode VM, and Native x86-64 executables.

## Verification
- Clean pure-Rust pipeline (no LLVM, no Cranelift, no C transpilation).
- 100% test coverage across unit tests, backend consistency tests, and example suites.
