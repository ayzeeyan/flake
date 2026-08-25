# Flake v0.7.3 Release Notes

Theme: **Harden the v0.7 leap — correctness, reliability, and performance**

Flake v0.7.3 is a dedicated hardening, optimization, and bug-fix release focusing on multi-target stability, concurrency runtime correctness, ownership tracking precision, and safe semantics-preserving optimizations.

## Highlights

### 1. Multi-Target Native Hardening
- **Pure-Rust ELF64 Generator**:
  - Dynamic `p_flags` and `phdr_count` calculation in `flake-codegen/src/elf.rs`, ensuring exact ELF64 specification compliance for binaries with or without `.rodata` data segments.
- **AArch64 Machine Code Assembler**:
  - Hardened opcode encodings and operand range verification for ALU operations, branches, and register manipulation.
  - Added structural ELF header and ARM64 instruction opcode regression tests.

### 2. Concurrency Runtime Hardening & Parity
- **State Machine Parity**:
  - Fixed VM `task_status` returning `"ready"` instead of `"completed"`, achieving 100% status string consistency across Interpreter, VM, and Native x86-64 backend.
- **Recursive Sendability Validation**:
  - Enforced compile-time inspection to recursively detect and reject nested references across `spawn` boundaries (e.g. lists or maps containing borrowed references).

### 3. Ownership & Lifetime Analysis Precision
- **Match Arm Isolation**:
  - Verified pattern binding movement isolation across `match` arms, guaranteeing independent branches while forbidding use-after-move within arm scopes.
- **Structural Borrow Protection**:
  - Prohibited moving or invalidating root structures while subfield references remain borrowed.

### 4. Optimizer Safety & IEEE-754 Compliance
- **Type-Aware Algebraic Reductions**:
  - Restricted integer identities to `IrType::Int` and boolean identities to `IrType::Bool`, guaranteeing full IEEE-754 semantics (`NaN * 0 = NaN`) and preventing incorrect constant foldings.
  - Preserved runtime division-by-zero checks.

### 5. Verification
- 100% test pass rate across all 9 workspace crates.
- Complete 3-way backend consistency verified across Tree-walking Interpreter, Bytecode VM, and Native x86-64 executable backend.
