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
  when you need it. Target native code and WebAssembly.

## Philosophy

1. Clarity above cleverness.
2. Progressive complexity — easy things should be easy, hard things should be possible.
3. Safety by default, without forcing full ownership ceremony on every program.
4. Effects are part of the type system and should be explicit.
5. Performance matters. Zero-cost abstractions are a goal.
6. Excellent error messages are non-negotiable.
7. The language should feel modern, readable, and distinctive.

## Status

v0.1 is under active development. See [ROADMAP.md](ROADMAP.md) for milestone
progress.

Current pipeline: **lexer → parser → tree-walking interpreter**, with a
bytecode VM planned next and a Cranelift/LLVM backend later.

## Build

```bash
cargo build
```

The `flake` binary is produced by the `flake-cli` crate.

## Usage

```bash
flake --help
flake --version

flake run examples/hello.flk
flake check examples/hello.flk
flake repl
```

## Language sketch

```flake
fn load_config(path: String) -> Config / io + alloc {
    let content = read_file(path)
    parse_config(content)
}

fn add(a: Int, b: Int) -> Int {
    a + b
}
```

- File extension: `.flk`
- Braced blocks, immutable `let`, mutable `var`
- Effects appear after the return type, joined with `+`
- Comments: `//` and `/* */`
- String interpolation: `"Hello, {name}"`

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

## License

MIT
