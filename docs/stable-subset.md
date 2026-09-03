# Flake v0.12 stable subset

This is the language surface a self-hosted frontend (Phase 2 / v0.11 completed) and self-hosted checker (Phase 3 / v0.12 completed) rely on. Features outside this list exist in the tree but are not a stability promise.

## Stable

- Programs, `fn`, `struct`, `enum`, `type` aliases, `import` / `pub import`
- Gradual types (`dyn`), effects (`/ io + alloc + conc + panic`), ownership (`strict`, `owned`, `ref`, `&` / `&mut`)
- Control flow: `if`, `while`, `for`, `loop`, `match`, `return`, `break`, `continue`
- Parametric polymorphism: `fn id[T](x: T) -> T`, generic structs/enums/aliases
- Marker traits and bounds: `trait Eq {}`, `impl Eq for Point {}`, `fn max[T: Ord](a: T, b: T) -> T`
- Trait methods and declarations: `trait Show { fn show(self) -> String }`
- Trait method implementations: `impl Show for Point { fn show(self) -> String { ... } }`
- Usable bounds and method dispatch on concrete types and generic type parameters (`T: Show` -> `value.show()`)
- Builtin bounds `Eq`, `Ord`, `Hash` (primitives implement them; `Ord` implies `Eq`)
- Algebraic enums, exhaustive `match`, Result-style `?`
- Modules, `pub` visibility, packages, `flake.toml`, deterministic `flake.lock`
- Structured concurrency: `spawn`, `await`, `nursery`, `Task[T]`, sendability of owned values (no borrowed `&` across `spawn`)
- Self-hosted frontend and checker modules (`selfhost/frontend/`): `span`, `tokens`, `lexer`, `ast`, `parser`, `check`, `scope`, `types`, `effects`, `ownership`, `main`
- Interpreter, bytecode VM, and native x86-64 Windows with matching results on this entire subset
- Native targets `x86_64-windows`, `x86_64-linux`, `aarch64-linux` for the existing pipeline
- Native `process.run` with stdout capture and exit code propagation matching Interpreter and VM

## Stdlib stable enough for tools

- `std/fs`: `read_to_string`, `write_string`, `read_dir`, `walk`, `read_lines`, `write_lines`, `append_string`, `exists`, `remove`, `file_size`, `is_directory`, `is_regular_file`
- `std/path`: `join_path`, `file_name`, `parent`, `extension`, `normalize`, `is_absolute`
- `std/process`: `program_args`, `current_dir`, `env_var`, `run`, `exit` (supported natively on Windows/Linux)
- `std/list`: generic `sort_items`, `find_eq`, `contains_eq`, `max_ord`, `min_ord`, plus existing dyn helpers
- `std/string`, `std/option`, `std/result`, `std/math`, `std/map`, `std/bytes`, `std/channel`

Program arguments are the builtin `args()` / `process.program_args()`, passed after `--` on `flake run`.

## Road to v1.0

1. **v0.10**: Trait methods (done)
2. **v0.11**: Self-hosted lexer + parser (done)
3. **v0.12**: Self-hosted checker (types, effects, ownership) (done)
4. **v0.13**: Native completeness + CTFE lite
5. **v0.14**: Bootstrap
6. **v1.0**: Freeze and ship

## Experimental (do not depend on for a self-hosted compiler yet)

- Trait default method bodies
- Associated types
- Specialization or overlapping impls
- Work-stealing / production async runtime
- Macros and CTFE lite (scheduled for v0.13)
- Public package registry
- New CPU targets beyond the three listed above

## Out of scope for v0.12

Emitting IR or native code from Flake (Phase 4/5); full CTFE (scheduled for v0.13); bootstrap (scheduled for v0.14). This release delivers the complete self-hosted type, effect, and ownership checker in Flake.
