# Flake v0.9 stable subset

This is the language surface a self-hosted frontend may rely on. Features
outside this list exist in the tree but are not a stability promise.

## Stable

- Programs, `fn`, `struct`, `enum`, `type` aliases, `import` / `pub import`
- Gradual types (`dyn`), effects (`/ io + alloc + conc + panic`), ownership (`strict`, `owned`, `ref`, `&` / `&mut`)
- Control flow: `if`, `while`, `for`, `loop`, `match`, `return`, `break`, `continue`
- Parametric polymorphism: `fn id[T](x: T) -> T`, generic structs/enums/aliases
- Marker traits and bounds: `trait Eq {}`, `impl Eq for Point {}`, `fn max[T: Ord](a: T, b: T) -> T`
- Builtin bounds `Eq`, `Ord`, `Hash` (primitives implement them; `Ord` implies `Eq`)
- Algebraic enums, exhaustive `match`, Result-style `?`
- Modules, `pub` visibility, packages, `flake.toml`, deterministic `flake.lock`
- Structured concurrency: `spawn`, `await`, `nursery`, `Task[T]`, sendability of owned values
- Interpreter, bytecode VM, and native x86-64 Windows with matching results on this subset
- Native targets `x86_64-windows`, `x86_64-linux`, `aarch64-linux` for the existing pipeline

## Stdlib stable enough for tools

- `std/fs`: `read_to_string`, `write_string`, `read_dir`, `walk`, `read_lines`, `write_lines`, `append_string`, `exists`, `remove`, `file_size`, `is_directory`, `is_regular_file`
- `std/path`: `join_path`, `file_name`, `parent`, `extension`, `normalize`, `is_absolute`
- `std/process`: `program_args`, `current_dir`, `env_var`, `run`, `exit`
- `std/list`: generic `sort_items`, `find_eq`, `contains_eq`, `max_ord`, `min_ord`, plus existing dyn helpers
- `std/string`, `std/option`, `std/result`, `std/math`, `std/map`, `std/bytes`, `std/channel`

Program arguments are the builtin `args()` / `process.program_args()`, passed after `--` on `flake run`.

## Experimental (do not depend on for a self-hosted compiler yet)

- Trait *methods* (v0.9 traits are marker bounds only)
- Specialization or overlapping impls
- Native `run_cmd` process capture (Interpreter/VM only; native returns an empty list)
- Work-stealing / production async runtime
- Macros and CTFE
- Public package registry
- New CPU targets beyond the three listed above

## Out of scope for v0.9

A full self-hosted Flake compiler. This release is preparation: bounds, stdlib depth, native quality, and this documented subset.
