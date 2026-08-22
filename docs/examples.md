# Examples guide

The examples are executable specifications for Flake's user-facing surface.
Every entry point in this guide is type-checked and its complete output is
compared across the tree-walking interpreter, bytecode VM, and native x86-64
backend.

## Run an example

```bash
flake run examples/task_pipeline.flk
flake run --vm examples/task_pipeline.flk
flake run --native examples/task_pipeline.flk
```

To keep a native executable rather than run a temporary one:

```bash
flake build examples/projects/release/main.flk -o release.exe
flake build examples/projects/release/main.flk -o release.exe --emit-asm
```

The first command writes only `release.exe`; the second also writes the
optional `release.s` diagnostic listing.

## Focused programs

| Example | Start here for |
| --- | --- |
| [`hello.flk`](../examples/hello.flk) | `main`, strings, and interpolation |
| [`fizzbuzz.flk`](../examples/fizzbuzz.flk) | loops, branches, and remainder |
| [`effects.flk`](../examples/effects.flk) | visible effect annotations |
| [`ownership.flk`](../examples/ownership.flk) | gradual and strict ownership |
| [`enum.flk`](../examples/enum.flk) | algebraic enums and exhaustive `match` |
| [`data.flk`](../examples/data.flk) | typed maps, scalar patterns, and Result-style `?` |
| [`concurrency.flk`](../examples/concurrency.flk) | the smallest `Task[T]`, `spawn`, and `await` program |
| [`task_pipeline.flk`](../examples/task_pipeline.flk) | several scope-bound tasks returning enum values |

`task_pipeline.flk` deliberately keeps child work pure. Its observable output
therefore agrees on the cooperative interpreter/VM and the native synchronous
fallback. Programs that print or mutate shared data inside a child can expose
the documented scheduling difference.

## Multi-file projects

### Inventory

[`projects/inventory/main.flk`](../examples/projects/inventory/main.flk) uses
project-rooted dotted imports, a public domain enum, qualified types, and a
service module with private helpers.

### Telemetry

[`projects/telemetry/main.flk`](../examples/projects/telemetry/main.flk) shows
transitive imports and proves that same-named private helpers remain isolated
inside their modules.

### Release gate

[`projects/release/main.flk`](../examples/projects/release/main.flk) combines
the v0.5 surface in one native-ready project:

```text
release/
├── main.flk
├── domain/
│   └── checks.flk
└── services/
    └── gate.flk
```

- `domain.checks` owns the public `Check` enum and private scoring policy.
- `services.gate` exposes a Result-like `GateResult`; `evaluate` uses `?` to
  propagate a rejected check.
- `main` schedules three pure evaluations, joins their typed task handles, and
  renders every enum variant exhaustively.
- A typed map supplies the scores, and the same source runs unchanged on all
  three backends.

### Batch processing pipeline

[`projects/pipeline/main.flk`](../examples/projects/pipeline/main.flk) demonstrates
a clean data processing pipeline across domain models, service transformations,
and output formatting:

```text
pipeline/
├── main.flk
├── domain/
│   └── record.flk
├── services/
│   └── transform.flk
└── utils/
    └── format.flk
```

- `domain.record` declares the `Record` entity and `Status` enum.
- `services.transform` processes records and produces `PipelineResult`.
- `utils.format` formats record summaries with pattern matching.
- Runs with identical results across Interpreter, Bytecode VM, and Native x86-64.

## Adding an example

Keep an example deterministic, give it a short comment explaining its teaching
goal, and add its entry point plus exact output to
`flake-cli/tests/examples.rs`. That suite automatically checks it and compares
the interpreter, VM, and native output. Backend-specific scheduling behavior
belongs in a focused test and should be called out in the source and docs.
