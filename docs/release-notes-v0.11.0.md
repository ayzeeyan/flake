# Flake v0.11.0 Release Notes: Self-Hosted Frontend

Flake v0.11.0 represents Phase 2 of 6 on the roadmap toward v1.0. It delivers a full self-hosted frontend written **purely in Flake**, capable of lexing, parsing, and AST dumping Flake source code across the Tree-walking Interpreter, Bytecode VM, and pure Rust Native x86-64 executable backend.

Workspace version: **0.11.0**.

---

## 1. Self-Hosted Token Types & Lexer (`selfhost/frontend/`)

- Modular scanner written entirely in Flake:
  - `span.flk`: Accurate 1-indexed source line, column, byte offset, and length tracking.
  - `tokens.flk`: Algebraic `TokenKind` enum representing all keywords, operators, and literals with formatters.
  - `lexer.flk`: Recursive string scanner handling comments (`//` and nested `/* */`), string escape sequences (`\n`, `\t`, `\r`, `\"`, `\\`, `\{`, `\}`), multi-character operators, identifiers, and numbers.

## 2. Complete Flake AST & Recursive-Descent Parser

- AST definitions and canonical S-expression serialization (`ast.flk`):
  - Strongly typed structures for items (`fn`, `struct`, `enum`, `trait`, `impl`, `type`, `import`), statements (`let`, `var`, `while`, `for`, `loop`, `expr`), expressions, and patterns.
  - Full support for gradual types, effect annotations, ownership modifiers (`strict`, `owned`, `ref`, `&`, `&mut`), and generic type bounds.
- Recursive-descent parser (`parser.flk`):
  - Precedence climbing across binary and unary operators.
  - Pattern matching with guards (`arm if cond => ...`), list patterns, and dotted enum variant paths.
  - Line-sensitive postfix disambiguation for calls and indexing.
  - Map literal `{ key: val }` and struct initialization disambiguation.
  - Result-based error propagation (`Result[T, String]`) with descriptive source span diagnostics.

## 3. Self-Hosted Frontend CLI Driver (`main.flk`)

- CLI interface supporting inspection, checking, and repository-wide walking:
  - `flake run selfhost/frontend/main.flk -- --tokens <file>`
  - `flake run selfhost/frontend/main.flk -- --ast <file>`
  - `flake run selfhost/frontend/main.flk -- --check <file...>`
  - `flake run selfhost/frontend/main.flk -- --walk <dir>`
  - `-h` / `--help` usage documentation.

## 4. Self-Parsing & 100% Repository Corpus Coverage

- Complete dogfooding and self-parsing verification:
  - Successfully scans and parses all 60 repository examples and projects (`--walk examples`).
  - Successfully parses the entire self-hosted compiler frontend itself (`--walk selfhost/frontend`).
  - Comprehensive golden integration tests in `flake-cli/tests/selfhost_frontend.rs`.
- Backend optimization:
  - Optimized O(1) ASCII and UTF-8 string indexing across Interpreter and Bytecode VM, eliminating quadratic memory allocations during large source scans.

---

## Road to v1.0

1. **v0.10.0**: Trait methods and usable bounds (done)
2. **v0.11.0**: Self-hosted frontend (lexer + parser in Flake) (done)
3. **v0.12.0**: Self-hosted checker (type, effect, and ownership analysis in Flake)
4. **v0.13.0**: Native completeness + CTFE lite
5. **v0.14.0**: Bootstrap (self-hosted frontend + checker compiling Flake)
6. **v1.0.0**: Freeze and ship
