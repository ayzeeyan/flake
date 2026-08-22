# Architecture

Flake is a Cargo workspace of small Rust crates. Every compilation and runtime
stage is owned by this repository: there is no LLVM, Cranelift, C
transpilation, or external code-generation framework.

```text
source files -> lexer -> parser + module graph -> AST -> type/effect/ownership check
                                                   |-> tree interpreter
                                                   |-> bytecode compiler -> VM
                                                   `-> custom CFG IR -> register allocation
                                                                    -> x86-64 encoder
                                                                    -> PE32+ writer
```

The interpreter and VM consume the language AST independently. The native
backend alone lowers through the custom IR. This separation keeps the
tree-walker readable as an executable model while making VM and native drift
visible in cross-backend tests.

## Workspace

| Crate | Responsibility |
| --- | --- |
| `flake-ast` | source spans, AST nodes, and shared source representation |
| `flake-lexer` | tokens, nested comments, and interpolated strings |
| `flake-parser` | recursive-descent/Pratt parser and module graph loader |
| `flake-types` | gradual types, effects, ownership, visibility, and diagnostics |
| `flake-interpreter` | tree-walking runtime and REPL engine |
| `flake-vm` | AST-to-bytecode compiler, stack VM, and cooperative tasks |
| `flake-ir` | typed control-flow-graph IR for native lowering |
| `flake-codegen` | liveness/register allocation, x86-64 encoding, runtime, and PE writing |
| `flake-cli` | `run`, `check`, `repl`, `ir`, and `build` commands |

## Frontend and modules

The module loader resolves the entry file into an acyclic graph before checking
or execution. `import services.checkout` maps to
`services/checkout.flk` beneath the entry file's project root;
single-segment imports first check next to the importer. Standard-library
lookup walks ancestor `std/` directories and can use `FLAKE_STD`.

Resolved paths become canonical dotted identities, and import edges retain the
actual target rather than relying on a file stem. Only explicit `pub`
declarations cross module boundaries. Namespaces always work; a bare export is
available only when exactly one import owns its spelling. Qualified values,
types, constructors, and enum patterns share these rules.

The interpreter creates one lexical environment per module. VM and native
functions use the same canonical qualification, preventing private helpers or
equal file stems in different directories from colliding. See
[modules.md](modules.md) and the
[release project](../examples/projects/release/main.flk).

## Values and control flow

Enums lower to tagged aggregate values. The interpreter retains a dedicated
enum representation; VM and native layouts use a tag followed by payload
fields. Exhaustive enum and scalar `match` expressions become comparisons and
branches. Result-style `?` inspects the tag, extracts the success payload, or
returns the unchanged error value early.

Maps carry concrete key/value types through checking and IR. Interpreter and
VM use typed keys; native code selects String or scalar comparison in the
in-tree linear-map runtime. All paths present entries in typed-key order.
Native insertion maintains that order while growing, so construction,
replacement, lookup, assignment, membership, and display remain deterministic.

Function references retain parameter, return, and effect types. Native direct
and indirect calls share Windows x64 ABI staging, including arguments after the
four register positions. CFG liveness and interference coloring reuse
callee-saved registers for non-overlapping locals; remaining locals use frame
slots.

## Tasks

`spawn` evaluates and captures a callable plus its arguments in a task owned by
the current function invocation. Interpreter and VM record pending children,
drive a child at `await`, and drain unawaited children before a successful
return. The VM represents the protocol with `Spawn`, `ReadyTask`, and `Await`
bytecode.

IR/native lowering intentionally erases the wrapper: `spawn call()` becomes
the call result and `await task` becomes that result. This v0.5 synchronous
fallback preserves pure values and native compilability without claiming that
the PE runtime has a scheduler. See [concurrency.md](concurrency.md).

## Diagnostics and consistency

The CLI checks source before normal execution and every native build;
`--skip-check` is an explicit debugging escape hatch for `run`. Diagnostics use
miette labels and help text for ownership, match coverage, variants, imports,
visibility, and overloaded builtins. VM bytecode instructions retain their AST
span, so runtime failures point to the responsible expression.

The tree-walker is the clearest behavioral reference, but no backend is trusted
by convention alone. Exact-output feature cases and shared failure cases run on
interpreter, VM, and native. Scheduling behavior is compared between the two
cooperative backends, with the native fallback documented separately. See
[testing.md](testing.md).

## CLI paths

```bash
flake check file.flk
flake run file.flk             # tree-walking interpreter
flake run --vm file.flk        # stack bytecode VM
flake run --native file.flk    # temporary native executable
flake ir file.flk              # dump the custom native IR
flake build file.flk -o out.exe
flake build file.flk -o out.exe --emit-asm
```

`flake build` writes only the PE executable unless `--emit-asm` is requested.
Output is staged beside its target and installed only after a successful write
and flush; replacement failures restore the previous executable.
