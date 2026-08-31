# Self-Hosted Flake Frontend (Phase 2 of 6 toward v1.0)

Flake v0.11.0 introduces a complete self-hosted frontend written **purely in Flake** under `selfhost/frontend/`.

The frontend is executed by the Rust-hosted `flake` compiler across all runtime backends (Tree-walking Interpreter, Bytecode VM, and Native x86-64 executable).

## Architecture

The self-hosted frontend is structured into modular Flake components:

```
selfhost/frontend/
├── span.flk      # Source locations (line, col, offset, len)
├── tokens.flk    # TokenKind enum and Token struct definitions
├── lexer.flk     # Scanner emitting structured tokens from raw source strings
├── ast.flk       # Complete Flake AST definitions and S-expression dumper
├── parser.flk    # Recursive-descent parser with precedence climbing
└── main.flk      # CLI driver with tokenizing, checking, AST dumping, and recursive walk
```

### 1. Source Spans (`span.flk`)
Tracks 1-indexed source line, column, byte offset, and length.
- `struct Span { line: Int, col: Int, offset: Int, len: Int }`
- `fn format_span(span: Span) -> String` produces `"line:col"` diagnostics.

### 2. Tokens & Scanner (`tokens.flk`, `lexer.flk`)
- Full token set covering all keywords (`fn`, `let`, `var`, `struct`, `enum`, `trait`, `impl`, `type`, `if`, `else`, `while`, `for`, `loop`, `match`, `return`, `break`, `continue`, `spawn`, `await`, `nursery`, `import`, `pub`, `strict`, `owned`, `ref`, `mut`, `dyn`, `true`, `false`, `nil`, `as`, `Task`), literals (integers, floats, escape-decoded strings, identifiers), and operators.
- Comment skipping: `//` single-line and nested `/* ... */` multi-line comments.
- Character string scanning with string escapes (`\n`, `\t`, `\r`, `\"`, `\\`, `\{`, `\}`).
- Multi-character operator disambiguation (`==`, `!=`, `<=`, `>=`, `&&`, `||`, `->`, `=>`, `..`, `+=`, `-=`, `*=`, `/=`, `%=`).

### 3. AST Definitions & Formatting (`ast.flk`)
- Complete data types for all Flake grammar constructs:
  - `TypeExpr`: `Named`, `ListType`, `OptionalType`, `TaskType`, `FnType`, `DynType`.
  - `Item`: `Fn`, `Struct`, `Enum`, `Trait`, `Impl`, `TypeAlias`, `Import`.
  - `Pattern`: `Wildcard`, `Literal`, `Ident`, `Variant`, `ListPat`.
  - `Expr`: `IntLit`, `FloatLit`, `StringLit`, `BoolLit`, `NilLit`, `Ident`, `Binary`, `Unary`, `Call`, `Spawn`, `Await`, `Try`, `Field`, `Index`, `Assign`, `If`, `Match`, `Block`, `Nursery`, `StructInit`, `ListLit`, `MapLit`, `Return`, `Break`, `Continue`.
  - `Stmt`: `Let`, `Var`, `While`, `For`, `Loop`, `Expr`.
  - `Program`: Top-level list of items.
- Canonical S-expression formatters: `dump_program`, `dump_item`, `dump_stmt`, `dump_expr`, `dump_pattern`, `dump_type`.

### 4. Recursive-Descent Parser (`parser.flk`)
- Full recursive descent parser supporting top-level item declarations, statements, pattern matching with guards, list patterns, and expression parsing.
- Operator precedence hierarchy (assignment → `||` → `&&` → equality → comparison → range `..` → term `+`/`-` → factor `*`/`/`/`%` → unary → postfix calls, indices, field accesses, try `?`).
- Delimited comma-separated and newline-separated parsing for function parameters, type parameters, struct fields, enum variants, list literals, map literals, and match arms.
- Dotted identifier resolution for qualified imports (`import a.b.c as d`), qualified types (`module.Type`), and qualified variant patterns (`module.Enum.Variant`).

### 5. CLI Driver (`main.flk`)
- `flake run selfhost/frontend/main.flk -- --tokens <file>`: Lexes and prints structured token stream.
- `flake run selfhost/frontend/main.flk -- --ast <file>`: Parses and dumps canonical S-expression AST.
- `flake run selfhost/frontend/main.flk -- --check <file...>`: Verifies syntax without AST dump.
- `flake run selfhost/frontend/main.flk -- --walk <dir>`: Recursively scans and parses all `.flk` files.

## Self-Parsing & Verification

The self-hosted frontend achieves 100% self-parsing and complete repository coverage:
- Successfully parses all 60 examples and project suites in the repository (`--walk examples`).
- Successfully parses all modules of `selfhost/frontend/` themselves (`--walk selfhost/frontend`).
- Golden test parity against expected AST outputs in `flake-cli/tests/selfhost_frontend.rs`.
