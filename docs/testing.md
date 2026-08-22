# Testing and backend consistency

Flake treats the tree-walking interpreter as the clearest executable model,
then requires the bytecode VM and native x86-64 backend to agree with it on
observable language behavior. Milestone 5 turns that policy into a dedicated
integration suite instead of relying only on examples and backend-local tests.

## Running the suites

```bash
cargo test --workspace
cargo test -p flake-cli --test backend_consistency
cargo test -p flake-cli --test examples
cargo clippy --workspace --all-targets -- -D warnings
```

The native parity tests build and execute PE32+ x86-64 programs, so the full
three-backend suite currently runs on Windows. Lexer, parser, checker,
interpreter, VM, IR, and assembly-generation unit tests remain ordinary Rust
tests in their owning crates.

## Test layers

- Frontend unit tests cover tokens, parsing, AST shapes, type/effect rules,
  ownership, module resolution, and diagnostic help.
- Runtime unit tests cover interpreter and VM control flow, values, builtins,
  structured task scopes, Result propagation, maps, and failure behavior.
- IR and codegen tests pin lowering types, CFG shapes, register allocation,
  ABI behavior, runtime helpers, PE metadata, and native process errors.
- Example tests type-check every checked-in example and compare its complete
  stdout across the interpreter, VM, and native backend.
- The table-driven consistency matrix runs focused programs for Float and
  checked-integer operations, typed list display, ordered maps, strings,
  structs, enums and scalar patterns, Result `?`, indirect wide calls,
  recursion, short-circuiting, ranges, and task results. Every program has an
  exact expected stdout.
- Failure-matrix tests require all backends to reject assertions, missing map
  keys, failing child tasks, and malformed builtin calls with the same semantic
  markers even when backend presentation differs.

Temporary integration-test sources use process- and counter-qualified names,
which keeps parallel test execution isolated and removes files after each run.

## Consistency contract

For a program accepted by the checker, the three execution paths should agree
on values, mutations, printed formatting, and whether execution succeeds.
Map display uses deterministic typed-key order. Builtin overloads such as
`range(a, b)`, optional-message `assert`, and variadic numeric `min`/`max` are
checked before execution so unsupported forms cannot drift between runtimes.

One scheduling-visible exception is intentional in v0.5: interpreter and VM
tasks are deferred cooperatively, while native `spawn` remains a synchronous
fallback. Pure task results must still agree across all backends. Ordering and
single-join behavior are compared directly between the two cooperative
backends. See [concurrency.md](concurrency.md).

VM instructions retain their originating AST span. Runtime diagnostics such as
division by zero therefore highlight the failing expression rather than the
start of the source file.

## Adding a language feature

A feature is not backend-complete until it has:

1. frontend acceptance and rejection tests;
2. interpreter and VM behavior tests;
3. IR/native coverage where the feature is supported;
4. a focused consistency-matrix case with exact output; and
5. failure tests when it introduces a new runtime error boundary.

If native behavior must remain partial, document the boundary explicitly and
keep accepted programs deterministic on every backend where parity is claimed.
