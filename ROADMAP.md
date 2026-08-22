# Flake Roadmap

**v0.5 is in development.** Milestones 1–4 are complete. Typed, scope-bound
tasks make `conc` operational; Result-style `?`, scalar patterns, stronger enum
checking, and typed maps expand the core language; and the owned x86-64 backend
now has CFG-aware register reuse, typed indirect calls, stronger native runtime
coverage, and more reliable executable production. Project-rooted dotted
imports, strict `pub` APIs, canonical module identities, and isolated module
environments now support comfortable hierarchical multi-file projects.

There is no LLVM, Cranelift, or C transpilation.

## v0.5 milestones

| # | Milestone | Status |
| --- | --- | --- |
| 1 | Structured concurrency foundations (`conc` effect) | **done** |
| 2 | Core language expansion (enums, pattern matching, errors, maps) | **done** |
| 3 | Native backend maturation | **done** |
| 4 | Strengthened module system | **done** |
| 5 | Cross-backend consistency and expanded testing | planned |
| 6 | v0.5 polish, documentation, and examples | planned |
| 7 | Flake v0.5 complete | planned |

## What v0.5 milestone 1 delivers

- `spawn call(args...)` and joining syntax `await task`
- Typed, non-escaping, single-use `Task[T]` handles
- `conc` effect checking on task creation and joins, while preserving child
  effects such as `io`
- Function-owned task scopes: explicit join, implicit join before successful
  return, deterministic failure propagation, and no detached work
- Cooperative task execution in both the tree-walking interpreter and VM
- VM bytecode support through `Spawn`, `ReadyTask`, and `Await`
- Native x86-64 remains usable through a documented synchronous fallback
- Concurrency tests, [model documentation](docs/concurrency.md), and
  [example](examples/concurrency.flk)

The milestone deliberately does not add a parallel scheduler, event loop,
timers, cancellation API, or native task runtime.

## What v0.5 milestone 2 delivers

- Result-like enums gain postfix `?`: `Ok(value)` unwraps and `Err(error)`
  returns immediately from a function with the same Result-like return type
- Literal patterns for `nil`, bools, integers, floats, and strings, including
  exhaustive Bool matching
- Stronger match diagnostics for duplicate/unreachable arms, wrong enum
  patterns, missing variants, and invalid variant arity
- Enum declaration validation and checked explicit `return` values
- Maps with String, Int, or Bool keys; runtime key types no longer collide in
  the interpreter or VM
- Map membership through `contains(map, key)`, deterministic interpreter/VM
  display, and native lookup/update/membership/display for all concrete key types
- Native/IR coverage for Result propagation, scalar matching, and concrete map
  key/value types
- Expanded `std/result.flk`, dedicated [error documentation](docs/errors.md),
  and the cross-backend [data example](examples/data.flk)

## What v0.5 milestone 3 delivers

- CFG liveness analysis, interference construction, and hotness-guided greedy
  coloring onto callee-saved GPRs; non-overlapping locals reuse registers while
  simultaneous aggregate/call operands remain distinct
- Typed function-address IR and real first-class native function values,
  including local and imported functions, typed return values, and indirect
  Windows-ABI calls with stack arguments beyond the fourth parameter
- Native Float negation and deterministic decimal formatting for `print`,
  `str`, and interpolation instead of integer truncation
- Stronger map runtime behavior: concrete String/Int/Bool/Float value display
  and an explicit runtime error for a missing key
- PE32+ headers with accurate code/data sizes and virtual section sizes
- Staged executable replacement, reliable temp cleanup, and preserved native
  stdout/stderr on process failure
- `flake build` type-checks before code generation, emits only the requested
  `.exe` by default, and writes a `.s` listing only with `--emit-asm`
- Focused regression coverage for register reuse, aggregate interference,
  indirect calls, maps, floats, PE metadata, and CLI artifact behavior

## What v0.5 milestone 4 delivers

- Project-rooted dotted imports: `import services.checkout` resolves to
  `services/checkout.flk`, while simple imports retain sibling-first behavior
- Path-derived canonical module identities, so equal file stems in different
  directories stay distinct through checking, VM compilation, IR, and native code
- Private-by-default module APIs: only explicit `pub` functions, structs,
  enums, and type aliases are visible to importers, and public signatures
  cannot leak private local types
- Qualified value, type, and enum-pattern access, plus bare imported names only
  when their owner is unambiguous
- Deterministic diagnostics for duplicate aliases/paths, alias/declaration
  conflicts, ambiguous exports, missing files, and full import-cycle chains
- Per-module interpreter environments that isolate private helpers; VM and
  native functions use the same canonical module qualification
- Hierarchical [inventory](examples/projects/inventory/main.flk) and transitive
  [telemetry](examples/projects/telemetry/main.flk) projects, exercised across
  the interpreter, VM, and native backend
- Dedicated [module documentation](docs/modules.md) covering layout,
  resolution, visibility, namespaces, and current package boundaries

## v0.4 milestones (complete)

| # | Milestone | Status |
| --- | --- | --- |
| 1 | High-quality native x86-64 backend | **done** |
| 2 | Mature gradual ownership | **done** |
| 3 | Language and module expansion | **done** |
| 4 | Standard library maturity | **done** |
| 5 | Cross-backend consistency and diagnostics | **done** |
| 6 | Polish, documentation and examples | **done** |
| 7 | Flake v0.4 complete | **done** |

## What v0.4 milestone 1 delivers

- Linear-scan-style register allocation onto Windows callee-saved GPRs
- Solid Windows x64 ABI (home space, stack args, callee-saved save/restore)
- Native floats via SSE2; low-level indirect-call encoding via `call r64`
- Existing examples still match the interpreter on `flake run --native`

## What v0.4 milestone 2 delivers

- Temporary borrows (`print(&x)`) end after the statement
- Assignment is forbidden while a value is borrowed at all, not only `&mut`
- Ownership model documented in [docs/ownership.md](docs/ownership.md)

## What v0.4 milestone 3 delivers

- `enum` declarations with unit and tuple variants
- `match` expressions with qualified variant patterns, binds, `_`, and exhaustiveness checking
- Module visibility: if a file uses `pub`, only `pub` items are exported
- Interpreter, VM, and native paths all run enums and `match`
- Examples: [enum](examples/enum.flk), [visible](examples/visible.flk)

## What v0.4 milestone 4 delivers

- Prelude natives: `trim`, `upper`, `lower`, `file_exists`, `env`, `cwd`, `remove_file`
- `std/` modules: `list`, `string`, `math`, `option`, `result` (prelude + explicit imports)
- Native-path support for the new natives
- Example: [stdlib](examples/stdlib.flk)

## What v0.4 milestone 5 delivers

- Cross-backend snippet tests (interpreter, VM, native) plus every example
- `help:` notes for non-exhaustive `match`, unknown variants, missing exports, and similar names
- Private-item errors suggest marking the declaration `pub`

## What v0.4 milestone 6 delivers

- Docs (README, tour, architecture, codegen, ownership, grammar) describe v0.4
- Example: [app](examples/app.flk) — enums, stdlib, native-ready

## What v0.4 delivers

- Linear-scan register allocation and a solid Windows x64 ABI
- SSE2 floats, indirect-call encoding, enums/`match`, modules with `pub`
- Stronger gradual ownership that stays optional
- Prelude + `std/` (`list`, `string`, `math`, `option`, `result`)
- Interpreter, VM, and native agreement on examples and snippets
- Still 100% pure Rust with a fully owned compiler pipeline

## v0.3 milestones

| # | Milestone | Status |
| --- | --- | --- |
| 1 | Expanded native x86-64 backend | **done** |
| 2 | Modules and imports | **done** |
| 3 | Stronger gradual ownership | **done** |
| 4 | Standard library expansion | **done** |
| 5 | Better diagnostics and comprehensive tests | **done** |
| 6 | Polish, documentation and examples | **done** |
| 7 | Flake v0.3 complete | **done** |

## What v0.3 milestone 1 delivers

- Native path covers the majority of the language used by current examples:
  control flow, functions (including 5+ arguments), locals, arithmetic, strings,
  structs, lists, maps, ranges, interpolation, and `print`
- Built-ins lowered natively: `len`, `push`, `pop`, `join`, `split`, `abs`,
  `min`, `max`, `range`, `str`, `int`, `type_of`, `assert`, `read_file`
- `flake run --native` matches the interpreter on every current example
- Still no LLVM, Cranelift, or C transpilation

## What v0.3 milestone 2 delivers

- `import math` loads sibling `math.flk`; `import math as m` binds a namespace
- Qualified calls (`math.add`) and unambiguous bare names both work
- Type checker, interpreter, VM, and native path all understand modules
- Example: [examples/modules.flk](examples/modules.flk)

## What v0.3 milestone 3 delivers

- Clearer ownership diagnostics with `help:` notes
- Borrows last until the end of the current block (then the value may be used again)
- Moving an `owned` value inside a loop is rejected
- If/else: a value is moved after the `if` only if both branches move it
- Unannotated code is unchanged

## What v0.3 milestone 4 delivers

- Prelude natives: `write_file`, `contains`, `starts_with`, `ends_with`, `first`, `last`
- Flake modules under `std/` (`list`, `string`), found by walking up from the importer
- Example: [examples/stdlib.flk](examples/stdlib.flk)

## What v0.3 milestone 5 delivers

- `help:` notes in ownership errors are shown as miette help text
- Missing `import` names the module and search path
- Tests cover write/read file, stdlib natives, missing modules, and
  interpreter / VM / native agreement on every example

## What v0.3 milestone 6 delivers

- README, tour, architecture, and codegen docs describe native code, modules,
  ownership, and the stdlib
- Examples: [modules](examples/modules.flk), [stdlib](examples/stdlib.flk),
  [borrow](examples/borrow.flk)

## v0.2 milestones

| # | Milestone | Status |
| --- | --- | --- |
| 1 | Bytecode VM feature parity | **done** |
| 2 | Custom intermediate representation | **done** |
| 3 | Native x86-64 codegen foundation | **done** |
| 4 | Expanded native code generation | **done** |
| 5 | Improved gradual ownership | **done** |
| 6 | Polish, tests, documentation and examples | **done** |
| 7 | Flake v0.2 complete | **done** |

## What v0.2 delivers

- `flake run --vm` runs every example (matches the tree-walker)
- Custom CFG IR (`flake ir file.flk`)
- Native path: `flake run --native` / `flake build` → PE32+ x86-64
  (functions, integers, strings, control flow, structs, lists, `print`)
- Stronger gradual ownership: borrows (`&` / `&mut`) in `strict` contexts,
  exclusive mutable borrows, no move-while-borrowed; ordinary code still
  needs no annotations
- Docs: [IR](docs/ir.md), [codegen](docs/codegen.md), updated tour and architecture

## v0.1 (complete)

Lexer, parser, AST, gradual types/effects/ownership, interpreter, VM
foundation, REPL. See git history milestones 0–10.

## Later / outside v0.5

1. Full lifetime/borrow checker on the level of Rust
2. aarch64 and System V ELF objects
3. Package manager / versioned dependencies / lockfile
4. Full parallel async runtime, scheduler, I/O reactor, and cancellation API
5. Self-hosting
6. Inlining and other optimisations beyond the current CFG-aware register allocator
7. Inline modules, public re-exports, and package-level dependency aliases
