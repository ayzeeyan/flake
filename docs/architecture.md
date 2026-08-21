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
| `flake-vm` | Bytecode compiler and stack VM |
| `flake-codegen` | Pure-Rust x86-64 encoder and PE writer |
| `flake-cli` | `flake` CLI: `run`, `check`, `repl`, `ir`, `build` |

The CLI type-checks before running (pass `--skip-check` to bypass).

```bash
flake run file.flk            # interpreter
flake run --vm file.flk       # bytecode VM
flake run --native file.flk   # compile + execute native image
flake build file.flk -o out.exe
flake ir file.flk             # dump IR
```
