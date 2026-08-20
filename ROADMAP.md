# Flake Roadmap

v0.1 goal: a coherent language with a tree-walking interpreter, a bytecode VM
foundation, gradual typing, a basic effect system, and optional ownership
annotations. Native/WASM backends come later.

## v0.1 milestones

| # | Milestone | Status |
| --- | --- | --- |
| 0 | Project skeleton and workspace | **done** |
| 1 | Lexer | **done** |
| 2 | Parser and AST | **done** |
| 3 | Basic tree-walking interpreter | **done** |
| 4 | Type system foundation | **done** |
| 5 | Effect system | pending |
| 6 | Gradual ownership foundation | pending |
| 7 | REPL and language expansion | pending |
| 8 | Bytecode VM foundation | pending |
| 9 | Polish, tests, documentation and examples | pending |
| 10 | Flake v0.1 complete | pending |

## Milestone notes

### 0 — Project skeleton

Cargo workspace, `flake` CLI with version/help, README, this roadmap.

### 1 — Lexer

Keywords, identifiers, literals, operators, strings, comments. Test suite.

### 2 — Parser and AST

Recursive-descent parser, AST, functions, variables, expressions, control flow,
effect annotations, pretty-printer.

### 3 — Tree-walking interpreter

`flake run` on basic programs, runtime errors.

### 4 — Type system foundation

Type representation, checking, local inference, `dyn` gradual typing.

### 5 — Effect system

Effect tracking, declarations on functions, basic checking.

### 6 — Gradual ownership foundation

Ownership/borrow annotations, checking in `strict` / `owned` contexts.
Ordinary code still runs without annotations.

### 7 — REPL and language expansion

`if`/`else`, loops, arrays/lists, basic standard library, interactive REPL,
rich diagnostics.

### 8 — Bytecode VM foundation

Stack-based VM, AST → bytecode, `flake run` can use the VM.

### 9 — Polish

Large test suite, non-trivial examples, language tour, cleanup.

### 10 — Finalization

Review, roadmap update, push to remote.

## After v0.1 (not in this release)

- Full ownership and borrowing
- Cranelift or LLVM backend
- Package manager
- Async / structured concurrency as a core effect
- Self-hosting
