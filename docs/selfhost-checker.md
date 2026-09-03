# Self-Hosted Flake Type, Effect, and Ownership Checker (Phase 3 of 6 toward v1.0)

Flake v0.12.0 introduces a complete self-hosted type, effect, and ownership checker written **purely in Flake** under `selfhost/frontend/`.

The self-hosted checker consumes the AST produced by the self-hosted parser, performs full semantic analysis conforming to the [Flake Stable Subset](stable-subset.md), reports diagnostics with exact file, line, and column source spans, and runs consistently across all three Flake backends: **Tree-walking Interpreter**, **Bytecode VM**, and **Native x86-64**.

---

## Architecture

The checker is organized into modular components under `selfhost/frontend/`:

```
selfhost/frontend/
├── check.flk      # Checker driver, Pass 1 (collection) & Pass 2 (resolution + type inference)
├── scope.flk      # Lexical scope stack, symbol table, builtin symbols
├── types.flk      # Type representation, trait definitions, impl registry, bound checking
├── effects.flk    # Effect collection, effect inference, and effect set verification
├── ownership.flk  # Strict/owned move tracking, escaping ref prevention, spawn sendability
├── span.flk       # Source span positions (line, column, byte offset, length)
├── tokens.flk     # Lexical tokens and enum definitions
├── lexer.flk      # Source scanner
├── ast.flk        # AST node definitions
├── parser.flk     # Recursive-descent parser
└── main.flk       # CLI entrypoint (--tokens, --ast, --check, --walk)
```

### 1. Scope and Name Resolution (`scope.flk`)
- Maintains a hierarchical stack of scopes (`ScopeStack`) with enter/exit block semantics.
- Resolves local variables, function parameters, loop variables, top-level functions, structs, enums, variants, and module imports.
- Flags undefined identifiers, duplicate definitions in the same scope, and shadow rules.
- Registers standard built-in functions, primitive types, and prelude symbols.

### 2. Type Checking and Trait Dispatch (`types.flk`)
- Represents Flake types: `Named`, `Param`, `Dyn`, `FnTy`, `List`, `Map`, `Optional`, `Task`, `Ref`.
- Tracks trait declarations (`TraitDef`) and implementations (`ImplDef`).
- Verifies trait method calls against trait bounds (`T: Show` $\to$ `x.show()`) and primitive impls (`Eq`, `Ord`, `Hash` for `Int`, `Float`, `String`, `Bool`).
- Enforces structural compatibility for assignments, return statements, function call arguments, list elements, and map keys/values.
- Type inference for local `let`/`var` bindings, binary operations, if/else expressions, and match patterns.

### 3. Effect System (`effects.flk`)
- Tracks algebraic effects: `io`, `alloc`, `conc`, `panic`, and `pure`.
- **Effect Inference**: Unannotated functions (`fn foo() { ... }`) infer their effect set from their constituent expressions and calls.
- **Effect Checking**: Explicitly annotated functions (e.g. `/ pure` or `/ io + alloc`) verify that all performed effects are declared in their effect clause.
- Pure functions cannot perform unannounced side effects or call functions requiring `io`/`alloc`/`conc`.
- Top-level `main` allows standard application effects (`io`, `alloc`, `conc`, `panic`).

### 4. Gradual Ownership and Sendability (`ownership.flk`)
- Gradual ownership checking in `strict` and `owned` functions:
  - Linear move tracking: detects use-after-move for consumed parameters and variables.
  - Escaping reference prevention: rejects returning references to local stack-allocated variables (`return &x`).
- Concurrency sendability:
  - Enforces that asynchronous tasks launched via `spawn` cannot capture borrowed references (`&x`, `&mut x`) across task boundaries.

### 5. Constant Checking and CTFE Lite (`check.flk`)
- Resolves top-level `const NAME: T = <expr>` declarations.
- Enforces strict purity for `const fn`: rejects impure effects (`/ io`, `/ conc`).
- Validates constant expressions (`ensure_const_expr`): permits only literals, arithmetic, comparisons, logic, if/else with mandatory else, block expressions, and calls to declared `const fn` functions.
- Rejects non-constant expressions, statement blocks, and runtime I/O inside const contexts.

### 6. Multi-File Module Resolution (`check.flk`)
- Resolves relative project module imports (`import domain.unit as unit` $\to$ `domain/unit.flk`).
- Resolves sibling module imports (`import secretmath` $\to$ `secretmath.flk`).
- Resolves standard library imports (`import fs`, `import list`, `import result` $\to$ `std/*.flk`).
- Exports only declarations explicitly marked `pub` from imported modules.

---

## Running the Checker

The checker is invoked through the self-hosted frontend CLI driver:

### 1. Single or Multi-File Checking
```bash
# Tree-walking Interpreter
flake run selfhost/frontend/main.flk -- --check examples/hello.flk

# Bytecode VM
flake run --vm selfhost/frontend/main.flk -- --check examples/hello.flk examples/traits.flk

# Native x86-64 executable
flake run --native selfhost/frontend/main.flk -- --check examples/projects/v09_flk_scan/main.flk
```

### 2. Standalone Native Binary
The selfhost frontend can be built as a standalone native binary without needing Rust or the Flake VM at check time:
```bash
flake build selfhost/frontend/main.flk -o flake-check-selfhost.exe
./flake-check-selfhost.exe --check examples/const_fold.flk
./flake-check-selfhost.exe --walk examples
```
Cross-compilation across the target matrix is supported:
```bash
flake build selfhost/frontend/main.flk --target x86_64-linux -o flake-check-selfhost-linux
flake build selfhost/frontend/main.flk --target aarch64-linux -o flake-check-selfhost-aarch64
```

### 3. Recursive Directory Walk
```bash
# Scans and type-checks all 62 .flk files in the examples tree
flake run selfhost/frontend/main.flk -- --walk examples
```

---

## Comparison with Rust Host Checker (`flake check`)

The Rust host compiler (`flake-types`) remains the authoritative compiler of record during Phase 3 and Phase 4. The self-hosted checker is tested against a defined golden corpus to guarantee semantic parity:

| Feature | Rust Host (`flake check`) | Self-Hosted (`--check`) | Parity Status |
| :--- | :--- | :--- | :--- |
| **Diagnostic format** | `{file}:{line}:{col}: {msg}` | `{file}:{line}:{col}: {msg}` | Identical format |
| **Accept corpus** | 100% accepted | 100% accepted | 16/16 golden files agree |
| **Reject corpus** | 100% rejected | 100% rejected | All negative cases agree |
| **Const validation** | Folds & checks purity | Rejects impure const calls | Parity verified |
| **Multi-file resolution** | Sibling, nested, `std/` | Sibling, nested, `std/` | Parity verified |
| **Sendability** | Disallows `&` across `spawn` | Disallows `&` across `spawn` | Parity verified |
| **Strict moves** | Rejects use-after-move | Rejects use-after-move | Parity verified |
| **Local ref escape** | Rejects `return &local` | Rejects `return &local` | Parity verified |
| **Dynamic escape hatch** | `dyn` permitted | `dyn` permitted | Gradual typing supported |

### Intentional Differences

1. **Diagnostic Phrasing**:
   - Host: `cannot find value \`foo\``
   - Self-host: `undefined variable \`foo\``
   - Both report the exact same source line and column.
2. **Whole-Program Type Inference**:
   - Host `flake check` performs global Hindley-Milner unification with type variables across all expressions.
   - Self-host checker implements flow-sensitive local inference for declarations, calls, and patterns, with `dyn` as a sound fallback for unannotated complex polymorphism, consistent with the gradual typing design of Flake.
3. **Execution Runtime**:
   - Host `flake check` is compiled Rust code integrated into the `flake` binary.
   - Selfhost `--check` is written in Flake itself, proving that the Flake language and compiler pipeline are capable of hosting complex language tools.
