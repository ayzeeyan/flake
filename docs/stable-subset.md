# Flake v0.13 stable subset

This is the language surface a self-hosted frontend (Phase 2 / v0.11 completed), self-hosted checker (Phase 3 / v0.12 completed), and native systems / CTFE lite compiler (Phase 4 / v0.13 completed) rely on. Features outside this list exist in the tree but are not a stability promise.

## Stable

- Programs, `fn`, `struct`, `enum`, `type` aliases, `import` / `pub import`
- Const items and CTFE lite:
  - `const NAME: T = <expr>` evaluated and folded at check/IR time
  - `const fn` pure functions evaluated at compile time without I/O or side-effects
  - Constant folding for integer/float/bool arithmetic, comparisons, logic, if/else branches, string concatenation, and string interpolation
  - Fuel and recursion depth limits (`CTFE_FUEL = 10_000`, `MAX_CALL_DEPTH = 256`) guaranteeing compile-time termination
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
- Self-hosted frontend compiles and runs natively (`flake build selfhost/frontend/main.flk -o flake-check-selfhost.exe`)
- Interpreter, bytecode VM, and native backends with identical results on this entire subset
- Native target matrix:
  - `x86_64-windows`: PE32+ binary format with Win32 runtime (`KERNEL32.dll`)
  - `x86_64-linux`: Standalone ELF64 binary format with direct Linux syscall runtime (`syscall` instruction, no Win32 IAT)
  - `aarch64-linux`: Standalone ELF64 binary format; partial target (AArch64 instruction encodings; systems APIs are explicit stubs; tests skip cleanly when no runner is available)
- Native systems APIs: `process.run`, `process.program_args`, `process.current_dir`, `fs.read_dir`, `fs.walk`

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
4. **v0.13**: Native completeness + CTFE lite (done)
5. **v0.14**: Bootstrap
6. **v1.0**: Freeze and ship

## Explicitly out of scope (macros are OUT)

- **Macro systems**: Macro expansion, token trees, hygiene, and procedural AST manipulation are explicitly out of scope.
- **Compile-time I/O**: CTFE cannot perform disk I/O, network requests, or process execution.
- **Arbitrary compile-time execution**: Only pure functions and constant expressions can be evaluated at compile time.
- Trait default method bodies
- Associated types
- Specialization or overlapping impls
- Work-stealing / production async runtime
- Public package registry
- New CPU targets beyond x86_64 and AArch64

