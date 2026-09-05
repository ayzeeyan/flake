# Flake v1.1.0 Release Notes: Native Speed and Memory

**Fast and small native code, same language.**

Flake v1.1.0 is the first performance and memory release following the v1.0.0 freeze. Guided by the `prog-benchy` STANDARD benchmark suite, Flake v1.1.0 makes the native compiler competitive in speed and memory footprint while maintaining 100% checksum parity, strict adherence to the v1.0 frozen language specification, and pure-Rust host compiler architecture (no LLVM, no Cranelift, no C transpilation).

Workspace version: **1.1.0**.

---

## 1. Motivation: The prog-benchy Diagnosis

In prog-benchy STANDARD benchmarks on Windows x86-64, Flake v1.0.0 passed with 100% checksum correctness on all five workloads (`nqueens`, `binarytrees`, `mandelbrot`, `nbody`, `spectralnorm`), but identified key optimization opportunities:
- **Peak RSS**: `binarytrees` allocated hundreds of thousands of small enum nodes and list headers without recycling, leading to high peak memory (~925 MB).
- **Float Operations**: Iterative floating-point calculations suffered from unnecessary GPR <-> XMM domain crossings and indirect math dispatch.
- **List Indexing**: List lookups went through general-purpose dynamic path dispatch rather than specialized array indexing.

Flake v1.1.0 targets these exact bottlenecks through value representation specialization and runtime allocation recycling.

---

## 2. Core Optimizations in v1.1.0

### 1. Standard Benchmark Suite & Runtime Instrumentation (`--stats`)
- Added standard reproductions of all five prog-benchy benchmarks under `examples/bench/`:
  - `binarytrees.flk`: Deep recursion, tree allocation, traversal, and memory footprint.
  - `mandelbrot.flk`: Complex iteration, inner float loops, set membership counting.
  - `nbody.flk`: Multi-body gravitational simulation, vector arithmetic, 3D math.
  - `nqueens.flk`: Backtracking search, recursive board placement, solution counting.
  - `spectralnorm.flk`: Matrix-vector multiplication, 2-norm eigenvalue approximation.
- Added `--stats` runtime instrumentation flag to `flake run` for accurate, truthful reporting of wall time and peak heap memory across all backends.
- Ensured truthful version reporting matching `CARGO_PKG_VERSION`.

### 2. Dense Typed List Indexing & Initial Capacity Optimization
- Specialized `IrType::List(_)` indexing in `flake-codegen` with direct memory load instructions (`mov rax, [rax + 16]; mov rax, [rax + 8 * idx]`) and branch-efficient bounds checks.
- Reduced initial empty list allocation padding from 16 elements to 4, preventing memory bloat on small arrays.
- `nbody.flk` native execution dropped from 2.088 s to 0.570 s (>3.6x speedup).

### 3. Segregated Free-List / Slab Allocator Recycling
- Implemented an in-binary segregated free-list allocator with 11 power-of-two size classes (16B, 24B, 32B, 48B, 64B, 128B, 256B, 512B, 1024B, 2048B, 4096B).
- Both Windows PE (`runtime.rs`) and Linux ELF (`runtime_linux.rs`) runtimes maintain live memory statistics and immediately reuse freed memory slots without OS heap overhead.
- Fixed stack initialization in native allocator to guarantee safe allocation sizing regardless of call-frame depth.

### 4. Direct SSE2 Float Codegen & Math Builtin
- First-class `sqrt` math builtin added across typechecker, IR lowerer, interpreter, VM, and native codegen.
- Lowered `sqrt` natively to the hardware SSE2 `sqrtsd` instruction (`sqrtsd xmm0, xmm0` or memory operand `sqrtsd xmm0, [rbp - slot]`).
- Implemented in-line `abs` for floating-point values using `andpd` with bitmask generated via `pcmpeqd` and `psrlq`.
- Added direct memory-operand SSE2 instructions (`addsd_xmm0_rbp`, `subsd_xmm0_rbp`, `mulsd_xmm0_rbp`, `divsd_xmm0_rbp`), eliminating tens of millions of redundant `movq` GPR <-> XMM domain transfers in hot loops.

### 5. Compact Enums & Immediate Sentinels
- **Immediate Sentinels for Nullary Variants**: Unit variants with no fields (e.g., `Tree.Empty`, `Option.None`) are represented as immediate unboxed sentinels `(tag << 1) | 1`. They require **0 heap allocations** and **0 bytes of memory**.
- **Contiguous Payload Structs**: Variants with fields (e.g., `Tree.Node(left, right, val)`) are laid out as contiguous single blocks `{ tag, fields... }` (32 bytes for `Tree.Node`) rather than separate 24-byte list headers and 32-byte element buffers (56 bytes).
- Introduced dedicated IR instructions: `Inst::MakeEnum`, `Inst::GetEnumTag`, and `Inst::GetEnumField`.
- Pattern matching extracts tags with immediate bit checks (`test rax, 1; sar rax, 1` vs `mov rax, [rax]`) with zero heap access for sentinels.

---

## 3. Benchmark Verification & Memory Results

Benchmarks executed natively on Windows x86-64 with `--stats`:

| Benchmark | Output / Checksum | Native Time | Peak Heap |
| :--- | :--- | :---: | :---: |
| **`nbody`** (N=50000) | `energy(end): -0.169050762382409` | **0.570 s** | **880 B** |
| **`spectralnorm`** (N=100) | `spectralnorm(N=100): 1.274219991234931` | **2.042 s** | **6.30 KB** |
| **`mandelbrot`** (N=200) | `in-set count: 15909` | **3.844 s** | **336 B** |
| **`nqueens`** (N=10) | `solutions: 724` | **4.233 s** | **544 B** |
| **`binarytrees`** (N=12) | `long lived tree: 12274` | **0.726 s** | **20.59 MB** |

### Memory Reduction Highlight
- **`binarytrees.flk`**:
  - Baseline v1.0.0: **924.58 MB**
  - Milestone 3 (free-list recycling): **72.34 MB**
  - Milestone 5 (immediate sentinels + 32B tree nodes): **20.59 MB**
  - **Total Reduction: >44x lower peak memory!**

---

## 4. Stability and Parity Guarantees

- **100% Output Parity**: All 5 benchmark workloads and test suites pass with identical output across the Tree-Walking Interpreter, Bytecode VM, and Native x86-64 codegen.
- **Zero Breaking Surface Changes**: Fully backward-compatible with all valid Flake v1.0.0 code.
- **Pure-Rust Compiler**: Zero external C/LLVM dependencies retained.
