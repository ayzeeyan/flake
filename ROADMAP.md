# Flake Roadmap

**v0.1 is complete.** v0.2 is in progress: full VM parity, a custom IR, and a
pure-Rust x86-64 backend.

## v0.2 milestones

| # | Milestone | Status |
| --- | --- | --- |
| 1 | Bytecode VM feature parity | **done** |
| 2 | Custom intermediate representation | **done** |
| 3 | Native x86-64 codegen foundation | **done** |
| 4 | Expanded native code generation | **done** |
| 5 | Improved gradual ownership | **done** |
| 6 | Polish, tests, documentation and examples | pending |
| 7 | Flake v0.2 complete | pending |

## v0.1 milestones

| # | Milestone | Status |
| --- | --- | --- |
| 0 | Project skeleton and workspace | **done** |
| 1 | Lexer | **done** |
| 2 | Parser and AST | **done** |
| 3 | Basic tree-walking interpreter | **done** |
| 4 | Type system foundation | **done** |
| 5 | Effect system | **done** |
| 6 | Gradual ownership foundation | **done** |
| 7 | REPL and language expansion | **done** |
| 8 | Bytecode VM foundation | **done** |
| 9 | Polish, tests, documentation and examples | **done** |
| 10 | Flake v0.1 complete | **done** |

## What v0.1 delivers

- `flake run examples/hello.flk` (tree-walker) and `flake run --vm examples/hello.flk`
- `flake check` with gradual typing, effect checking, and optional ownership
- `flake repl` with persistent bindings
- Effect annotations (`/ io + alloc`) parsed and checked
- `dyn` for gradual typing; omitted types inferred locally
- `strict` / `owned` functions enforce move semantics; ordinary code does not
- miette diagnostics
- Examples, language tour, and a crate layout ready for a native backend

## After v0.1

These are explicitly out of this release:

1. **Full ownership and borrowing** — lifetimes, reborrows, and non-lexical
   scopes, still gradual.
2. **Cranelift or LLVM backend** — compile the existing AST/bytecode to native
   code and WebAssembly.
3. **Package manager** — modules, `import` that actually loads files, a lockfile.
4. **Async / structured concurrency** as a core `conc` effect.
5. **Self-hosting** — a Flake compiler written in Flake.

Near-term VM work (still post-v0.1): compile `for`, structs, maps, `break` /
`continue`, and compound assignment so `--vm` covers the full interpreter
language.
