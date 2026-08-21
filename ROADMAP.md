# Flake Roadmap

**v0.2 is complete.** The language has interpreter/VM parity, a custom IR, and a
pure-Rust x86-64 backend that emits Windows PE executables. There is no LLVM,
Cranelift, or C transpilation.

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

## Later (not in v0.2)

1. Full lifetime/borrow checker on the level of Rust
2. aarch64 and System V ELF objects
3. Package manager / real `import`
4. Async / structured concurrency as a `conc` effect
5. Self-hosting
6. Heavy optimisations (register allocation, inlining)
7. Native maps and indirect calls
