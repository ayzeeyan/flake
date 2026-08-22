# Flake language tour

**Clarity, crystallized.**

This is the v0.5 development tour. Flake is a braced, immutable-by-default language with
local type inference, an explicit `dyn` escape hatch, effect annotations,
opt-in ownership, enums and `match`, multi-file `import`, and a standard
library that runs on the interpreter, VM, and native x86-64 backend. v0.5 adds
typed structured tasks under `conc`, Result propagation, stronger patterns,
and typed-key maps.

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
`Map[K, V]`, `Range`, `Task[T]`, `dyn`.

```flake
let n: Int = 42
let xs = [1, 2, 3]
let box: dyn = true
let also: Int = box   // ok: dyn is consistent with every type
```

`dyn` is gradual typing: it is *consistent* with any type, not equal to it.
Known mismatches still fail (`let x: Int = true` is an error).

Structs, enums, and type aliases:

```flake
struct Point { x: Int y: Int }
type Name = String

enum Color {
    Red
    Green
    Rgb(Int, Int, Int)
}

let c = Color.Rgb(1, 2, 3)
let name = match c {
    Color.Red => "red"
    Color.Green => "green"
    Color.Rgb(r, g, b) => "rgb {r},{g},{b}"
}
```

`match` is an expression. Variant patterns must be qualified (`Color.Red`). A
`_` or identifier arm is a catch-all. Matches on enums must cover every variant
(or include `_`). Literal patterns support `nil`, bools, integers, floats, and
strings; matching a Bool with both `true` and `false` is exhaustive. Duplicate
patterns and arms after a catch-all are rejected as unreachable. Enums and
scalar patterns work on the interpreter, VM, and native paths.

User-defined enums cover Result-style error handling:

```flake
enum Result { Ok(Int) Err(String) }

fn add_two(result: Result) -> Result {
    let value = result?
    Result.Ok(value + 2)
}
```

`?` unwraps `Ok(value)` and immediately returns `Err(error)` from a function
returning the same Result-like enum. Result-like enums have exactly
`Ok(value)` followed by `Err(error)`. See [errors.md](errors.md).

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

## Structured concurrency

`spawn` starts a call in the current function's task scope. `await` joins its
single-use `Task[T]` handle and yields `T`:

```flake
fn square(n: Int) -> Int { n * n }

fn main() / conc + io {
    let left: Task[Int] = spawn square(6)
    let right = spawn square(7)
    print(await left)
    print(await right)
}
```

Both operations require `conc`; effects performed by the child call are also
required in the parent. Call arguments are evaluated and captured when the
task is spawned. A task cannot escape the spawning function, cannot be awaited
twice, and is implicitly joined before that function returns if it was not
awaited explicitly. Child failures propagate to the parent.

The interpreter and VM currently use deterministic cooperative execution; no
parallel scheduler or event loop is implied yet. The native backend can run
the same surface through a synchronous fallback, although scheduling-visible
side-effect order can differ. See
[concurrency.md](concurrency.md) for the complete milestone-1 model.

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

If the imported file has any `pub` item, only those items are exported
(qualified `math.add` and unambiguous bare names). If it has no `pub` at all,
everything is exported — existing modules keep working. Private helpers stay
callable inside their own file.

There is no package manager or versioned registry yet.

Standard library modules live under `std/` and are found by walking up from
the importer: `list`, `string`, `math`, `option`, `result`. Prelude natives
include `trim`, `upper`, `lower`, `file_exists`, `env`, `cwd`, and
`remove_file` in addition to I/O, lists, and strings.

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

`if` is an expression: `let x = if cond { 1 } else { 2 }`. So is `match`.

## Lists, maps, and strings

```flake
let xs = [1, 2, 3]
xs[0]
xs[-1]
push(xs, 4)
len(xs)

let m = { "a": 1, "b": 2 }
m["a"]
contains(m, "a")
m["c"] = 3

print("Hello, {name}!")   // interpolation
```

Map keys are typed and must be `String`, `Int`, or `Bool`. Key identity keeps
types distinct at runtime, and `contains(map, key)` probes without raising a
missing-key error. Lookup, assignment, membership, and concrete key types work
on all three execution paths.

## Standard library (v0.4)

Prelude natives (no `import`):

| Function | Role | Effects |
| --- | --- | --- |
| `print(...)` | write a line | `io` |
| `len(x)` | length of list/string/map | pure |
| `push(list, x)` | append | `alloc` |
| `pop(list)` | remove last | pure |
| `first`, `last` | ends of a list or string | pure |
| `str`, `int`, `float` | conversions | `alloc` for `str` |
| `type_of(x)` | runtime type name | `alloc` |
| `assert(cond, msg?)` | check a condition | `panic` |
| `read_file` / `write_file` | UTF-8 files | `io` (`+ alloc` for read) |
| `file_exists`, `remove_file` | path checks / delete | `io` |
| `env(name)`, `cwd()` | environment | `io` |
| `trim`, `upper`, `lower` | ASCII/Unicode string case and trim | `alloc` |
| `contains`, `starts_with`, `ends_with` | list/string/map search | pure |
| `abs`, `min`, `max` | numeric helpers | pure |
| `range(n)` / `range(a, b)` | integer range | pure |
| `join(list, sep)` | concatenate | `alloc` |
| `split(s, sep)` | split a string | `alloc` |

Flake modules under `std/` (walk up from the importer):

| Module | Contents |
| --- | --- |
| `list` | `is_empty`, `rest`, `reverse`, `concat`, `take`, `drop`, `sum` |
| `string` | `is_blank`, `surround`, `replace`, `repeat` |
| `math` | `clamp`, `pow`, `sign` (sibling `math.flk` wins if present) |
| `option` | `enum Option { Some(dyn) None }`, `is_some`, `unwrap_or` |
| `result` | `Result`, `is_ok`, `is_err`, `unwrap_or`, `error_or`, `unwrap` |

## Back ends

```bash
flake run examples/hello.flk            # tree-walking interpreter
flake run --vm examples/hello.flk       # bytecode VM (full language)
flake run --native examples/hello.flk   # x86-64 PE (Windows)
flake build examples/hello.flk -o hello.exe
flake build examples/hello.flk -o hello.exe --emit-asm
flake ir examples/hello.flk             # dump the custom IR
```

The VM and native backend match the interpreter on all examples. Native code
uses CFG-aware liveness/interference register allocation and the Windows x64
ABI. `flake build` emits only the executable unless `--emit-asm` is requested;
see [codegen.md](codegen.md).
