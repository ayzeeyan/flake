# Flake v0.9.0 Release Notes: Self-hosting preparation

Flake v0.9.0 is the release that makes it realistic to start writing large
compiler-like programs in Flake. It adds generic bounds, deepens the systems
stdlib, hardens native comparison of generic values, and documents a stable
subset. The pipeline remains 100% pure Rust: no LLVM, Cranelift, or C
transpilation.

Workspace version: **0.9.0**.

---

## 1. Generic bounds and marker traits

```flake
fn max[T: Ord](a: T, b: T) -> T {
    if a > b { a } else { b }
}

trait Show {}
impl Show for Int {}
impl[T: Eq] Eq for Box[T] {}
```

- Bounds on functions, structs, enums, and type aliases: `T: Eq + Hash`.
- Builtin bounds `Eq`, `Ord`, `Hash` (primitives; `Ord` implies `Eq`).
- User `trait` / `impl` declarations (empty marker bodies).
- Diagnostics for unknown traits, missing bounds, and unsatisfied impls.

See [examples/traits.flk](../examples/traits.flk).

## 2. Stdlib depth for tools

- `fs.read_dir`, `fs.walk`, `fs.read_lines`, `fs.write_lines`, `fs.append_string`,
  `fs.create_directory`, `fs.is_directory`, `fs.is_regular_file`.
- Missing files and empty/missing directories return `Result.Err` / `Option.None`.
- `args()` builtin and `process.program_args()`; `flake run file.flk -- a b`.
- `process.run` captures stdout/stderr/exit code on Interpreter and VM.
- Generic list helpers: `sort_items`, `find_eq`, `contains_eq`, `max_ord`, `min_ord`.

Native x86-64 implements `args`, `list_dir`, `is_dir`, `is_file`, `append_file`,
and `create_dir`. Native `run_cmd` is a stub (empty list).

## 3. Native quality

- String ordering uses `rt_strcmp`; equality uses `rt_streq`.
- Generic/`dyn` comparisons use `rt_val_cmp` so imported helpers like
  `list.sort_items(["zeta", "alpha"])` match Interpreter/VM.
- Leaf inlining budget is 32 instructions.

## 4. Ownership × generics × concurrency

- Inference keeps `ref` inside generic structs.
- `spawn` rejects generic values that contain references.
- Generic structs with bounds remain borrow-checked.
- Owned generic values can be spawned on all backends.

## 5. Stable subset and flagship example

- [docs/stable-subset.md](stable-subset.md) lists what a self-hosted frontend
  may rely on versus experimental bits.
- Flagship [v09_flk_scan](../examples/projects/v09_flk_scan/main.flk) scans
  `.flk`-shaped sources and reports fn/struct counts.

## Explicitly out of scope

Full self-hosted compiler, macros/CTFE, package registry, work-stealing async
runtime, new CPU targets, large syntax redesigns, and trait methods.

## Verification

- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- Interpreter, VM, and Native agree on traits, stdlib depth, and the flagship
  example.
