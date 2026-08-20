# Flake language tour

**Clarity, crystallized.**

This is the v0.1 tour. Flake is a braced, immutable-by-default language with
local type inference, an explicit `dyn` escape hatch, effect annotations, and
opt-in ownership.

## Hello

```flake
fn main() {
    let name = "World"
    print("Hello, {name}!")
}
```

```bash
flake run examples/hello.flk
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

## Running on the VM

```bash
flake run --vm examples/hello.flk
```

The default `flake run` uses the tree-walking interpreter. `--vm` selects the
stack-based bytecode VM (a subset of the language: `for`, structs, and maps are
not compiled yet).
