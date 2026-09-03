# Flake v0.12.0 Release Notes: Self-Hosted Checker

Flake v0.12.0 represents Phase 3 of 6 on the roadmap toward v1.0. It delivers a full self-hosted semantic checker written **purely in Flake**, capable of type checking, algebraic effect inference and verification, gradual ownership analysis, and concurrency sendability checking across the Tree-walking Interpreter, Bytecode VM, and pure Rust Native x86-64 executable backend.

Workspace version: **0.12.0**.

---

## 1. Self-Hosted Checker Architecture (`selfhost/frontend/`)

- Modular analysis pipeline written entirely in Flake:
  - `check.flk`: Two-pass semantic driver performing Pass 1 global collection and Pass 2 symbol resolution, type checking, and expression typing.
  - `scope.flk`: Lexical scope stack (`ScopeStack`), symbol table, variable shadowing rules, and prelude/builtin symbol registration.
  - `types.flk`: Internal type representations (`FlkType`), trait definitions, implementation registries, bound checking, and trait method dispatch.
  - `effects.flk`: Algebraic effect set analysis, effect inference for unannotated functions, and effect boundary validation.
  - `ownership.flk`: Gradual linear ownership checking in `strict`/`owned` functions, escaping local reference detection, and task `spawn` sendability rules.
  - `span.flk`: Accurate 1-indexed source line, column, byte offset, and length tracking for all diagnostics.

## 2. Type Checking, Generics, and Trait Bounds

- Strong type checking and type inference:
  - Flow-sensitive local inference for `let`/`var` bindings, binary operations, if/else expressions, and match patterns.
  - Trait definition and implementation registry supporting generic trait bounds (`T: Show` $\to$ `x.show()`).
  - Primitive trait implementations for `Eq`, `Ord`, and `Hash` across `Int`, `Float`, `String`, and `Bool`.
  - Gradual typing escape hatch: sound integration of `dyn` for dynamic interoperability.

## 3. First-Class Algebraic Effect Checking & Inference

- Verification of side-effect boundaries:
  - Supported algebraic effects: `io`, `alloc`, `conc`, `panic`, and `pure`.
  - **Effect Inference**: Unannotated functions (`fn foo() { ... }`) infer their required effects from their bodies and propagate them to callers.
  - **Effect Enforcement**: Explicitly annotated functions (`/ pure`, `/ io`, `/ alloc + io`) guarantee that undeclared effects cannot be hidden.
  - Top-level `main` automatically permits standard runtime effects.

## 4. Gradual Ownership and Concurrency Sendability

- Ownership safety in systems programming:
  - Linear move tracking: detects and rejects use-after-move in `strict` and `owned` contexts.
  - Local reference escaping prevention: rejects returning stack-allocated references (`return &x`).
  - Concurrency sendability: enforces that asynchronous `spawn` tasks cannot capture borrowed references (`&x` or `&mut x`) across task boundaries.

## 5. Multi-File Module Resolution & Enhanced Walk Mode

- Multi-module and project support:
  - Resolves dotted project submodules (e.g. `import domain.unit as unit` $\to$ `domain/unit.flk`).
  - Resolves sibling modules and standard library modules (`std/`).
  - Enforces `pub` visibility rules on imported items.
  - `--check <file...>` supports checking multiple files in a single invocation.
  - `--walk <dir>` recursively parses and type-checks directory trees, reporting file, line, and column error spans.

## 6. Parity with Host Compiler & 100% Corpus Verification

- Semantic equivalence:
  - 100% check pass rate on all 60 examples in the repository across Interpreter, Bytecode VM, and Native x86-64.
  - Automated golden corpus test suite (`flake-cli/tests/selfhost_frontend.rs`) validating that the Rust host compiler (`flake check`) and self-hosted checker agree on both accept and reject corpora.

---

## Road to v1.0

1. **v0.10.0**: Trait methods and usable bounds (done)
2. **v0.11.0**: Self-hosted frontend (lexer + parser in Flake) (done)
3. **v0.12.0**: Self-hosted checker (types, effects, ownership) (done)
4. **v0.13.0**: Native completeness + CTFE lite
5. **v0.14.0**: Bootstrap (self-hosted compiler producing executables)
6. **v1.0.0**: Freeze and ship
