# Architecture

Flake is a Cargo workspace of small crates. Every stage of the compiler is
owned by this repository: there is no LLVM, Cranelift, or C backend.

```
source → lexer → parser → AST
                       ↘ type / effect / ownership checker
                       ↘ IR
                          ↘ tree-walking interpreter
                          ↘ bytecode compiler → stack VM
                          ↘ x86-64 codegen → PE32+ executable
```

| Crate | Responsibility |
| --- | --- |
| `flake-ast` | Spans, AST, pretty-printer, diagnostic rendering |
| `flake-lexer` | Tokens, comments, string interpolation |
| `flake-parser` | Recursive-descent + Pratt parser, `import` module loader |
| `flake-types` | Inference, `dyn`, effects, gradual ownership |
| `flake-ir` | Control-flow-graph IR (locals + basic blocks) |
| `flake-interpreter` | Tree-walking runtime and REPL engine |
| `flake-vm` | Bytecode compiler, stack VM, and cooperative task opcodes |
| `flake-codegen` | Pure-Rust x86-64 encoder and PE writer |
| `flake-cli` | `flake` CLI: `run`, `check`, `repl`, `ir`, `build` |

`import name` loads `name.flk` next to the importer, then walks parent
directories for `std/name.flk`. If a module contains any `pub` item, only those
items are exported; otherwise everything is exported.

`enum` / `match` are lowered to tagged lists (`[tag, fields…]`) on the IR, VM,
and native paths. The interpreter keeps a dedicated enum value for display.

`spawn` captures a callable and its evaluated arguments in a task registered
to the current function invocation. Interpreter and VM task scopes drive a
pending child when it is `await`ed and drain remaining children before return.
The VM represents this directly with `Spawn`, `ReadyTask`, and `Await`
opcodes. For v0.5 milestone 1, IR/native lowering intentionally erases the
task wrapper and executes the call synchronously; this keeps native builds
coherent until a native task runtime exists.

The CLI type-checks before running (pass `--skip-check` to bypass).

```bash
flake run file.flk            # interpreter
flake run --vm file.flk       # bytecode VM
flake run --native file.flk   # compile + execute native image
flake build file.flk -o out.exe
flake ir file.flk             # dump IR
```

Diagnostics use miette. Messages may include a `help:` line (ownership,
non-exhaustive `match`, unknown variants, similar names, missing `pub`).
