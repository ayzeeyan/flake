# Flake language tour

**Clarity, crystallized.**

This is the v0.3 tour. Flake is a braced, immutable-by-default language with
local type inference, an explicit `dyn` escape hatch, effect annotations,
opt-in ownership, multi-file `import`, and a small standard library.

## Hello

```flake
fn main() {
    let name = "World"
    print("Hello, {name}!")
}
```

```bash
flake run examples/hello.flk
flake run --vm examples/hello.flk
flake run --native examples/hello.flk
flake check examples/hello.flk
flake repl
```

Statements are separated by newlines (semicolons are optional). `let` is
immutable; `var` is mutable.

## Functions

```flake
fn add(a: Int, b: Int) -> Int {
    a + b
}
```

Parameter and return types may be omitted. The checker infers them locally;
unconstrained bindings become `dyn`.

The last expression in a block is the function's value. Use `return` to leave
early.

## Types

Built-in types: `Int`, `Float`, `Bool`, `String`, `Nil`, `[T]` (lists),
`Map[K, V]`, `Range`, `dyn`.

```flake
let n: Int = 42
let xs = [1, 2, 3]
let box: dyn = true
let also: Int = box   // ok: dyn is consistent with every type
```

`dyn` is gradual typing: it is *consistent* with any type, not equal to it.
Known mismatches still fail (`let x: Int = true` is an error).

Structs and type aliases:

```flake
struct Point { x: Int y: Int }
type Name = String
```

## Effects

Effects appear after the return type, joined with `+`:

```flake
fn read_file(path: String) -> String / io + alloc {
    // ...
}

fn greet(name: String) / io {
    print("Hello, {name}!")
}

fn add(a: Int, b: Int) -> Int {
    a + b   // inferred pure
}
```

Declared effects must cover everything the body actually does. Unannotated
functions have their effects inferred. `main` may perform I/O without writing
`/ io`.

Known effects: `io`, `alloc`, `conc`, `panic`, `pure`.

## Modules

Each `.flk` file is a module. `import math` loads `math.flk` from the same
directory. `import math as m` binds the namespace under `m`.

```flake
import math

fn main() {
    print(math.add(2, 40))
    print(square(5))    // bare name, if unambiguous
}
```

Functions, structs, and type aliases in the imported file are exported.
There is no package manager or versioned registry yet.

## Ownership

Ordinary code does not need ownership annotations. In a `strict` or `owned`
function, `owned` values are moved on use and `ref` values may be reused.

```flake
strict fn consume(name: owned String) / io {
    print(name)
    // print(name)  // error: use of moved value
}

fn reuse(name: owned String) / io {
    print(name)
    print(name)     // ok outside strict
}
```

Copy types (`Int`, `Float`, `Bool`, `Nil`) never move.

Borrows (`&x` / `&mut x`) last until the end of the current block. Moving an
`owned` value inside a loop is an error. After `if`/`else`, a value is treated
as moved only if both branches moved it.

## Control flow

```flake
if n > 0 {
    print("pos")
} else if n == 0 {
    print("zero")
} else {
    print("neg")
}

while i < n {
    i = i + 1
}

for x in 0..n {
    print(x)
}

loop {
    break
}
```

`if` is an expression: `let x = if cond { 1 } else { 2 }`.

## Lists, maps, and strings

```flake
let xs = [1, 2, 3]
xs[0]
xs[-1]
push(xs, 4)
len(xs)

let m = { "a": 1, "b": 2 }
m["a"]

print("Hello, {name}!")   // interpolation
```

## Standard library (v0.1)

| Function | Role | Effects |
| --- | --- | --- |
| `print(...)` | write a line | `io` |
| `len(x)` | length of list/string/map | pure |
| `push(list, x)` | append | `alloc` |
| `pop(list)` | remove last | pure |
| `str`, `int`, `float` | conversions | `alloc` for `str` |
| `type_of(x)` | runtime type name | `alloc` |
| `assert(cond, msg?)` | check a condition | `panic` |
| `read_file(path)` | read a UTF-8 file | `io + alloc` |
| `abs`, `min`, `max` | numeric helpers | pure |
| `range(n)` / `range(a, b)` | integer range | pure |
| `join(list, sep)` | concatenate | `alloc` |
| `split(s, sep)` | split a string | `alloc` |

## Back ends

```bash
flake run examples/hello.flk            # tree-walking interpreter
flake run --vm examples/hello.flk       # bytecode VM (full language)
flake run --native examples/hello.flk   # x86-64 PE (Windows)
flake build examples/hello.flk -o hello.exe
flake ir examples/hello.flk             # dump the custom IR
```

The VM matches the interpreter on all examples. The native backend is a
pure-Rust x86-64 encoder; see [codegen.md](codegen.md).
