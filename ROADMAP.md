# Flake Roadmap

**v0.3 is in progress.** v0.2 delivered interpreter/VM parity, a custom IR, and a
pure-Rust x86-64 backend. v0.3 expands that backend until existing programs
compile to working native executables, then adds modules, stronger ownership,
and a practical standard library.

There is no LLVM, Cranelift, or C transpilation.

## v0.3 milestones

| # | Milestone | Status |
| --- | --- | --- |
| 1 | Expanded native x86-64 backend | **done** |
| 2 | Modules and imports | **done** |
| 3 | Stronger gradual ownership | **done** |
| 4 | Standard library expansion | **done** |
| 5 | Better diagnostics and comprehensive tests | pending |
| 6 | Polish, documentation and examples | pending |
| 7 | Flake v0.3 complete | pending |

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
