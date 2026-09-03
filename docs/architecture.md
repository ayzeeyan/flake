# Architecture

Flake is a Cargo workspace of small, focused Rust crates. Every compilation and runtime stage is owned by this repository: there is no LLVM, Cranelift, C transpilation, or external code-generation framework.

```text
source files -> lexer -> parser + module graph -> AST -> type/effect/ownership check
                                                   |-> tree interpreter
                                                   |-> bytecode compiler -> VM
                                                   `-> custom CFG IR -> register allocation
                                                                    -> machine code encoder (x86-64 / AArch64)
                                                                    -> binary writer (PE32+ / ELF64)
```

The interpreter and VM consume the language AST independently. The native backend lowers through the custom IR. This separation keeps the tree-walker readable as an executable reference model while making VM and native divergence immediately visible in cross-backend consistency tests.

Traits support marker bounds (`trait Eq {}`) and trait method declarations and implementations (`trait Show { fn show(self) -> String }`). Traits and generic bounds are verified in `flake-types` with static method resolution. Native comparison of generic and `dyn` values uses `rt_val_cmp` so `Eq`/`Ord` helpers agree identically with the interpreter and VM.

---

## Workspace

| Crate | Responsibility |
| :--- | :--- |
| `flake-ast` | Source spans, AST nodes, constant folding definitions, and shared source representation |
| `flake-lexer` | Tokens, nested comments, and interpolated string lexing |
| `flake-parser` | Recursive-descent Pratt parser, manifest parser, and module graph loader |
| `flake-types` | Gradual types, algebraic effects, ownership/borrowing, trait methods, CTFE lite, and diagnostics |
| `flake-interpreter` | Tree-walking reference runtime and REPL engine |
| `flake-vm` | AST-to-bytecode compiler, stack VM, and cooperative task execution |
| `flake-ir` | Typed control-flow-graph IR and optimization passes for native lowering |
| `flake-codegen` | Liveness/register allocation, x86-64/AArch64 encoding, Win32/Linux syscall runtimes, and PE/ELF writers |
| `flake-cli` | `run`, `build`, `check`, `ir`, `init`, `new`, `lock`, `update`, `repl`, and `bootstrap` commands |

---

## Frontend and Modules

The module loader resolves the entry file into an acyclic dependency graph before checking or execution. `import services.checkout` maps to `services/checkout.flk` beneath the entry file's project root; single-segment imports first check next to the importing file. Standard-library lookup walks ancestor `std/` directories and respects `FLAKE_STD`.

Resolved paths become canonical dotted identities, and import edges retain the actual target rather than relying on a file stem. Only explicit `pub` declarations cross module boundaries. Namespaces always work; a bare export is available only when exactly one import owns its spelling. Qualified values, types, constructors, and enum patterns share these rules.

The interpreter creates one lexical environment per module. VM and native functions use canonical symbol qualification, preventing private helpers or equal file stems in different directories from colliding. See [modules.md](modules.md) and the [release project](../examples/projects/release/main.flk).

---

## Values and Control Flow

Enums lower to tagged aggregate values. The interpreter retains a dedicated enum representation; VM and native layouts use a tag followed by payload fields. Exhaustive enum and scalar `match` expressions become comparisons and branches. Result-style `?` inspects the tag, extracts the success payload, or returns the unchanged error value early.

Maps carry concrete key/value types through checking and IR. Interpreter and VM use typed keys; native code selects string or scalar comparison in the in-tree linear-map runtime. All paths present entries in typed-key order. Native insertion maintains that order while growing, so construction, replacement, lookup, assignment, membership, and display remain deterministic.

Function references retain parameter, return, and effect types. Native direct and indirect calls share Windows x64 ABI staging, including arguments after the four register positions. CFG liveness and interference coloring reuse callee-saved registers for non-overlapping locals; remaining locals use frame slots.

---

## Tasks and Concurrency

`spawn` evaluates and captures a callable plus its arguments in a task owned by the current function invocation. Interpreter and VM record pending children, drive a child at `await`, and drain unawaited children before a successful return. The VM represents the protocol with `Spawn`, `ReadyTask`, and `Await` bytecode.

Flake IR represents tasks with `IrType::Task`, `Inst::Spawn`, and `Inst::Await`. On the native x86-64 backend, tasks are represented as heap-allocated task descriptors with state tracking (`Pending = 0`, `Joined = 1`, `Running = 2`, `Cancelled = 3`). Awaiting an already-joined task triggers a runtime error on all three backends, providing 100% single-join consistency. See [concurrency.md](concurrency.md).

---

## Optimizations

Flake IR passes (`flake-ir::opt`) optimize functions before code emission:
- **Constant folding and propagation**: Evaluates pure compile-time expressions with checked arithmetic overflow protection.
- **Unreachable block elimination**: Removes unreachable basic blocks via CFG reachability analysis.
- **Dead code elimination**: Prunes unused pure instructions.
- **Copy propagation**: Eliminates redundant moves between immutable locals.
- **Assembler peephole optimizations**: Redundant self-move elimination (`mov reg, reg`).

---

## Diagnostics and Consistency

The CLI checks source before normal execution and every native build; `--skip-check` is an explicit debugging escape hatch for `run`. Diagnostics use miette labels and help text for ownership, match coverage, variants, imports, visibility, and overloaded builtins. VM bytecode instructions retain their AST span, so runtime failures point to the responsible expression.

The tree-walker is the clearest behavioral reference, but no backend is trusted by convention alone. Exact-output feature cases and shared failure cases run on interpreter, VM, and native backends. Scheduling behavior is compared between the two cooperative backends, with the native fallback documented separately. See [testing.md](testing.md).

---

## CLI Commands

```bash
flake init [--name name]         # Initialize flake.toml package in current directory
flake new path/to/pkg            # Create new package directory
flake check [path]               # Type check file or package
flake run [path]                 # Run on tree-walking interpreter
flake run --vm [path]            # Run on stack bytecode VM
flake run --native [path]        # Compile and run native executable
flake ir [path]                  # Dump optimized custom IR
flake build [path] -o out.exe    # Build PE32+ or ELF64 executable
flake build [path] --emit-asm    # Build executable and emit human-readable .s assembly
flake lock                       # Generate or check deterministic flake.lock
flake update                     # Update dependencies in flake.lock
flake repl                       # Interactive Read-Eval-Print Loop
flake bootstrap [-v] [--keep]    # Run the full Stage 0-2 bootstrap verification loop
```
