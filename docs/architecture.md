# Architecture

Flake v0.1 is a Cargo workspace of small crates:

```
source  →  lexer  →  parser  →  AST
                              ↘ type checker (types, effects, ownership)
                              ↘ tree-walking interpreter
                              ↘ bytecode compiler → stack VM
```

| Crate | Responsibility |
| --- | --- |
| `flake-ast` | Spans, AST, pretty-printer, diagnostic rendering |
| `flake-lexer` | Tokens, comments, string interpolation |
| `flake-parser` | Recursive-descent + Pratt parser |
| `flake-types` | Inference, `dyn`, effects, gradual ownership |
| `flake-interpreter` | Tree-walking runtime and REPL engine |
| `flake-vm` | Bytecode compiler and stack VM |
| `flake-cli` | `flake` binary: `run`, `check`, `repl` |

The CLI type-checks before running (pass `--skip-check` to bypass). `flake run --vm`
compiles to bytecode instead of interpreting the AST.

Later backends (Cranelift or LLVM) should consume the same AST, or the bytecode
module, without rewriting the front end.
