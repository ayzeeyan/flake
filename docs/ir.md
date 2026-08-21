# Flake IR

The Flake intermediate representation is a **control-flow graph of basic
blocks** whose instructions read and write **local slots**.

It is the hand-off between the front end and every back end we own:

```
source → lexer → parser → AST → type/effect check
                              → IR
                                 ↘ bytecode VM (optional)
                                 ↘ x86-64 codegen
```

No LLVM, Cranelift, or foreign IR is involved.

## Design goals

1. **Simple to emit** from the AST (one local per binding or temporary).
2. **Simple to lower** to stack bytecode and to x86-64 stack frames.
3. **Explicit control flow.** The last instruction of every block is `Jump`,
   `Branch`, or `Return`.
4. **Carry language facts** that later passes need: effect sets, `strict` /
   `owned` flags, struct layouts.

SSA φ-nodes are deliberately omitted. Values that live across blocks are
ordinary locals (the x86-64 backend maps them to stack slots).

## Units

A **module** is a list of struct definitions and functions.

A **function** has:

- parameters (the first N locals)
- remaining locals (lets, vars, temporaries)
- an effect set (`io`, `alloc`, …) and whether it was specified
- `strict` / `owned` flags
- a list of **basic blocks**, with `bb0` as the entry

A **local** is `%id` with an optional source name and an IR type (`Int`,
`Bool`, `String`, `dyn`, …).

## Instructions (summary)

| Instruction | Meaning |
| --- | --- |
| `%d = const c` | load a literal |
| `%d = %s` | copy |
| `%d = add/sub/… %a, %b` | arithmetic / compare |
| `%d = call name(%a, …)` | static call (user fn or builtin) |
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
