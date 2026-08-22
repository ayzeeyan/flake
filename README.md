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

**Flake v0.5.1 is complete.** Building on v0.5's structured concurrency,
Result-style errors, and pure-Rust x86-64 code generator, v0.5.1 adds solid
incremental improvements and stability:
- **Native backend reliability**: resilient struct field initialization order,
  safe bounds checking for list and string indexing, and robust string comparisons.
- **Module system polish**: importer-relative and project-root-relative module
  resolution, full IR struct field type propagation, and a new multi-file
  [pipeline project](examples/projects/pipeline/main.flk).
- **Expanded standard library**: practical helpers across `std/list.flk`,
  `std/string.flk`, `std/math.flk`, `std/option.flk`, and `std/result.flk`.
- **Language additions**: built-in `keys(map)` and `values(map)` reflection,
  along with `contains(range, n)` for forward and reverse integer ranges.
- **No external codegen**: Pure Rust end-to-end (no LLVM, Cranelift, or C transpilation).

See [ROADMAP.md](ROADMAP.md) for milestone status and [docs/tour.md](docs/tour.md)
for a language tour. Backend consistency policy and test commands live in
[docs/testing.md](docs/testing.md); [docs/examples.md](docs/examples.md) is the
guided example index.

See the [v0.5.1 release notes](docs/release-notes-v0.5.1.md) for the complete
feature and compatibility summary.

## Build and run

```bash
cargo build
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

flake run examples/hello.flk
flake run --vm examples/hello.flk
flake run --native examples/hello.flk
flake run examples/modules.flk
flake run examples/stdlib.flk
flake run examples/enum.flk
flake run examples/data.flk
flake run examples/projects/pipeline/main.flk
flake run --native examples/projects/pipeline/main.flk
flake run examples/projects/inventory/main.flk
flake run --native examples/projects/telemetry/main.flk
flake run --vm examples/concurrency.flk
flake run --native examples/task_pipeline.flk
flake run --native examples/app.flk
flake run --native examples/projects/release/main.flk
flake build examples/hello.flk -o hello.exe
flake build examples/hello.flk -o hello.exe --emit-asm
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
| [examples/task_pipeline.flk](examples/task_pipeline.flk) | Portable structured tasks, enums, scalar patterns, and maps |
| [examples/projects/inventory/main.flk](examples/projects/inventory/main.flk) | Hierarchical imports, public enums, and qualified types |
| [examples/projects/telemetry/main.flk](examples/projects/telemetry/main.flk) | Transitive modules and isolated private helpers |
| [examples/projects/release/main.flk](examples/projects/release/main.flk) | Native-ready modules, tasks, Result propagation, and exhaustive patterns |

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
- [Modules and multi-file projects](docs/modules.md)
- [IR](docs/ir.md)
- [Native codegen](docs/codegen.md)
- [Grammar sketch](docs/grammar.md)
- [Architecture](docs/architecture.md)
- [Examples guide](docs/examples.md)
- [Testing and backend consistency](docs/testing.md)
- [Roadmap](ROADMAP.md)

## License

MIT
