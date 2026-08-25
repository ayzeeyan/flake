# Flake v0.7.4 Release Notes

Theme: **Small, high-quality increment**

Flake v0.7.4 is a focused, lightweight release delivering small quality-of-life improvements, standard library additions, backend optimizations, and additional showcase examples while preserving 100% pure Rust and fully owned compiler pipeline architecture.

## Highlights

### 1. Backend Code Generation Optimizations
- **AArch64 Immediate Materialization**:
  - Optimized 64-bit immediate lowering (`mov_i64` in `flake-codegen/src/aarch64.rs`) to emit `MOVK` instructions only for non-zero 16-bit halfwords, reducing instruction count for sparse 64-bit constants.
  - Added instruction-level encoding tests verifying minimal code size.

### 2. Standard Library Enhancements
- **Math Utilities**:
  - Added `hypot_sq(a, b) -> Int` and `dist_sq(x1, y1, x2, y2) -> Int` to `std/math.flk` for squared Euclidean geometry and distance computations.
- **String Utilities**:
  - Added `is_alphanumeric(ch: String) -> Bool` to `std/string.flk`.

### 3. Examples & Documentation
- **New Geometry Example**:
  - Added `examples/geometry.flk` demonstrating 2D shape representation with structs, enums, pattern matching, and geometric math helpers.
  - Added full test coverage in `flake-cli/tests/examples.rs` across all 3 execution backends.
- **Documentation Polish**:
  - Updated `docs/examples.md` index and roadmap tracking.

### 4. Quality & Compatibility
- 100% test pass rate across the workspace with 0 warnings in strict clippy mode.
- 100% backend consistency across Tree-walking Interpreter, Bytecode VM, and Native x86-64 executable backend.
