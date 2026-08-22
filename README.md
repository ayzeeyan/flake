# Flake

<p align="center">
  <img src="flakelogo.png" alt="Flake programming language logo" width="320">
</p>

**Clarity, crystallized.**

Flake is a safe, modern systems language with gradual ownership and a first-class
effect system that makes side effects visible and controllable.

```flake
fn main() {
    let name = "World"
    print("Hello, {name}!")
}
```

```bash
flake run examples/hello.flk
```

## Why Flake?

Most systems languages ask you to pay the full cost of safety up front. Most
dynamic languages hide cost and effects until they bite you. Flake sits in
between:

- **Strong static typing by default**, with local inference and an explicit `dyn`
  escape hatch for gradual typing.
- **Ownership and borrowing as opt-in annotations**, becoming stricter in
  `strict` / `owned` contexts instead of on every binding.
- **Effects as part of the type system.** If a function does I/O, you can see it
  in the signature: `fn read_file(path: String) -> String / io + alloc`.
- **Structured tasks as a visible effect.** `spawn work()` and `await task`
  produce typed, scope-bound `Task[T]` values under the `conc` effect.
- **Data and errors stay explicit.** Algebraic enums, exhaustive `match`,
  typed maps, and Result-style `?` propagation keep failure paths visible.
- **Progressive complexity.** Write a script. Drop down to fine-grained control
  when you need it.

## Philosophy

1. Clarity above cleverness.
2. Progressive complexity — easy things should be easy, hard things should be possible.
3. Safety by default, without forcing full ownership ceremony on every program.
4. Effects are part of the type system and should be explicit.
5. Performance matters. Zero-cost abstractions are a goal.
6. Excellent error messages are non-negotiable.
7. The language should feel modern, readable, and distinctive.

## Status

**v0.5 is in development.** Milestones 1–2 are complete: typed structured
tasks, Result-style `?` propagation, scalar and enum patterns, stronger match
diagnostics, and typed maps work across the interpreter, VM, and native
x86-64 path. Native concurrency retains its documented synchronous fallback.
No LLVM or Cranelift.

See [ROADMAP.md](ROADMAP.md) for milestone status and [docs/tour.md](docs/tour.md)
for a language tour.

## Build and run

```bash
cargo build
cargo test

flake run examples/hello.flk
flake run --vm examples/hello.flk
flake run --native examples/hello.flk
flake run examples/modules.flk
flake run examples/stdlib.flk
flake run examples/enum.flk
flake run examples/data.flk
flake run --vm examples/concurrency.flk
flake run --native examples/app.flk
flake build examples/hello.flk -o hello.exe
flake check examples/hello.flk
flake ir examples/hello.flk
flake repl
```

The `flake` binary comes from the `flake-cli` crate.

## Language sketch

```flake
fn load_config(path: String) -> Config / io + alloc {
    let content = read_file(path)
    parse_config(content)
}

fn add(a: Int, b: Int) -> Int {
    a + b
}

strict fn take(name: owned String) / io {
    print(name)
}

fn parallel_shape() -> Int / conc {
    let task: Task[Int] = spawn add(20, 22)
    await task
}

enum Result { Ok(Int) Err(String) }

fn add_two(result: Result) -> Result {
    let value = result?
    Result.Ok(value + 2)
}
```

- File extension: `.flk`
- Braced blocks, immutable `let`, mutable `var`
- Effects appear after the return type, joined with `+`
- Comments: `//` and `/* */`
- String interpolation: `"Hello, {name}"`

## Examples

| File | What it shows |
| --- | --- |
| [examples/hello.flk](examples/hello.flk) | Interpolation and `main` |
| [examples/fibonacci.flk](examples/fibonacci.flk) | Recursion |
| [examples/fizzbuzz.flk](examples/fizzbuzz.flk) | Loops and conditionals |
| [examples/effects.flk](examples/effects.flk) | Effect annotations |
| [examples/lists.flk](examples/lists.flk) | Lists and helpers |
| [examples/ownership.flk](examples/ownership.flk) | Gradual vs strict ownership |
| [examples/config.flk](examples/config.flk) | Structs |
| [examples/modules.flk](examples/modules.flk) | `import` |
| [examples/enum.flk](examples/enum.flk) | Enums, `match`, Result-style errors |
| [examples/visible.flk](examples/visible.flk) | `pub` module visibility |
| [examples/stdlib.flk](examples/stdlib.flk) | Standard library |
| [examples/borrow.flk](examples/borrow.flk) | Borrows |
| [examples/app.flk](examples/app.flk) | Native-ready mini program |
| [examples/concurrency.flk](examples/concurrency.flk) | Scope-bound `spawn` / `await` tasks |
| [examples/data.flk](examples/data.flk) | Result `?`, scalar patterns, and typed maps |

## Workspace

| Crate | Role |
| --- | --- |
| `flake-ast` | Abstract syntax tree and source spans |
| `flake-lexer` | Tokenizer |
| `flake-parser` | Recursive-descent parser |
| `flake-types` | Types, effects, and gradual ownership |
| `flake-interpreter` | Tree-walking interpreter |
| `flake-ir` | Custom CFG intermediate representation |
| `flake-vm` | Stack-based bytecode VM (interpreter parity) |
| `flake-codegen` | Pure-Rust x86-64 encoder and PE writer |
| `flake-cli` | `flake` command-line interface |

## Docs

- [Language tour](docs/tour.md)
- [Ownership](docs/ownership.md)
- [Structured concurrency](docs/concurrency.md)
- [Errors and Result propagation](docs/errors.md)
- [IR](docs/ir.md)
- [Native codegen](docs/codegen.md)
- [Grammar sketch](docs/grammar.md)
- [Architecture](docs/architecture.md)
- [Roadmap](ROADMAP.md)

## License

MIT
