# Flake

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

**v0.1 complete** — lexer, parser, type/effect/ownership checker, tree-walking
interpreter, bytecode VM foundation, and REPL.

See [ROADMAP.md](ROADMAP.md) for milestone status and [docs/tour.md](docs/tour.md)
for a language tour.

## Build and run

```bash
cargo build
cargo test

flake run examples/hello.flk
flake run --vm examples/hello.flk
flake check examples/hello.flk
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

## Workspace

| Crate | Role |
| --- | --- |
| `flake-ast` | Abstract syntax tree and source spans |
| `flake-lexer` | Tokenizer |
| `flake-parser` | Recursive-descent parser |
| `flake-types` | Types, effects, and gradual ownership |
| `flake-interpreter` | Tree-walking interpreter |
| `flake-vm` | Stack-based bytecode VM |
| `flake-cli` | `flake` command-line interface |

## Docs

- [Language tour](docs/tour.md)
- [Grammar sketch](docs/grammar.md)
- [Architecture](docs/architecture.md)
- [Roadmap](ROADMAP.md)

## License

MIT
