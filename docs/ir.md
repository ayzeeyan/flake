# Flake IR

The Flake intermediate representation is a **control-flow graph of basic
blocks** whose instructions read and write **local slots**.

It is the hand-off between the front end and Flake's native backend:

```
source → lexer → parser → AST → type/effect check
                         ├──→ tree interpreter
                         ├──→ bytecode compiler → VM
                         └──→ IR → register allocation → x86-64 codegen
```

The tree interpreter and bytecode compiler intentionally consume the AST
directly. No LLVM, Cranelift, or foreign IR is involved.

## Design goals

1. **Simple to emit** from the AST (one local per binding or temporary).
2. **Simple to lower** to x86-64 stack/register frames and runtime calls.
3. **Explicit control flow.** The last instruction of every block is `Jump`,
   `Branch`, or `Return`.
4. **Carry language facts** that later passes need: effect sets, `strict` /
   `owned` flags, struct layouts.

SSA φ-nodes are deliberately omitted. Values that live across blocks are
ordinary locals; the x86-64 backend uses CFG liveness to place them in
callee-saved registers or stack slots.

## Units

A **module** is a list of struct definitions and functions. Enums lower to
tagged lists (`[tag, field…]`) plus `match` as tag compares and branches.
Scalar patterns use direct comparisons. Result `?` reads the tag, extracts the
success payload on tag zero, and emits an early `Return` for the error branch.
Map constructors retain concrete key/value types when they can be inferred.

A **function** has:

- parameters (the first N locals)
- remaining locals (lets, vars, temporaries)
- an effect set (`io`, `alloc`, …) and whether it was specified
- `strict` / `owned` flags
- a list of **basic blocks**, with `bb0` as the entry

A **local** is `%id` with an optional source name and an IR type (`Int`,
`Bool`, `String`, `Map[K, V]`, `fn -> R`, `dyn`, …). Function values retain
their return type so an indirect call produces a correctly typed result.

In v0.5 milestone 1, structured task handles remain an interpreter/VM runtime
concept. IR lowering maps `spawn call()` to the call result and `await task` to
the underlying value. This is the native backend's deliberate synchronous
fallback, not a promise of native parallel scheduling.

## Instructions (summary)

| Instruction | Meaning |
| --- | --- |
| `%d = const c` | load a literal |
| `%d = fnaddr name` | materialize a user or imported function address |
| `%d = %s` | copy |
| `%d = add/sub/… %a, %b` | arithmetic / compare |
| `%d = call name(%a, …)` | direct call (user fn or builtin) |
| `%d = call %f(%a, …)` | indirect call through a function local |
| `%d = %o[%i]` / `%o[%i] = %v` | index |
| `%d = %o.field` / `%o.field = %v` | struct field |
| `%d = list […]` / `map` / `struct` / `range` | constructors |
| `%d = iter %s` / `%v, %m = iternext %i` | iteration |
| `%d = concat …` | string interpolation |
| `goto bbN` | jump |
| `br %c bbT, bbE` | conditional |
| `return %v` | return |

## Example

```flake
fn add(a: Int, b: Int) -> Int {
    a + b
}
```

lowers to approximately:

```
fn add(a: Int, b: Int) -> Int {
  bb0:
    %2 = add %0, %1
    return %2
}
```

Dump IR from the CLI:

```bash
flake ir examples/hello.flk
```
