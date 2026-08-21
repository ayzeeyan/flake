# Flake Roadmap

**v0.4 is in progress.** v0.3 delivered practical native x86-64, modules, stronger
ownership, and a small standard library. v0.4 raises the native backend to
production quality (register allocation, full ABI), matures ownership, and
expands the language surface.

There is no LLVM, Cranelift, or C transpilation.

## v0.4 milestones

| # | Milestone | Status |
| --- | --- | --- |
| 1 | High-quality native x86-64 backend | **done** |
| 2 | Mature gradual ownership | **done** |
| 3 | Language and module expansion | **done** |
| 4 | Standard library maturity | **done** |
| 5 | Cross-backend consistency and diagnostics | **done** |
| 6 | Polish, documentation and examples | pending |
| 7 | Flake v0.4 complete | pending |

## What v0.4 milestone 1 delivers

- Linear-scan-style register allocation onto Windows callee-saved GPRs
- Solid Windows x64 ABI (home space, stack args, callee-saved save/restore)
- Native floats via SSE2; indirect calls via `call r64`
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

## Later (not in v0.3)

1. Full lifetime/borrow checker on the level of Rust
2. aarch64 and System V ELF objects
3. Package manager / versioned dependencies / lockfile
4. Async / structured concurrency as a `conc` effect
5. Self-hosting
6. Heavy optimisations (register allocation, inlining)
7. Indirect calls and native floating-point
