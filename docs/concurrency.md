# Structured concurrency

Flake v0.5 milestone 1 makes concurrency a real part of the language surface
without pretending that a production async runtime already exists. The model
is deliberately small: typed tasks, explicit joins, lexical ownership, and a
visible `conc` effect.

## Surface

```flake
fn compute(n: Int) -> Int {
    n * 2
}

fn main() / conc + io {
    let task: Task[Int] = spawn compute(21)
    print(await task)
}
```

- `spawn f(args...)` requires a call expression and returns `Task[T]`, where
  `T` is the call's result type.
- `await task` joins that task and returns its `T` result. `await` is Flake's
  joining syntax; the existing `join(list, separator)` string helper is
  unrelated.
- `spawn` and `await` both perform the `conc` effect.
- Effects of the spawned call are preserved. Spawning an `/ io` function needs
  both `conc` and `io` in the parent.

## Structured lifetime

Every function invocation owns a task scope. A spawned task is registered in
that scope and its handle cannot escape through a known static type. On a
successful return, the runtime joins every child that was not explicitly
awaited, in spawn order. A child error becomes a parent error. If the parent is
already failing, pending work is abandoned with the scope.

Task handles are single-use. Awaiting a joined or cancelled handle is a runtime
error; `strict` ownership contexts can additionally diagnose repeated moves.
This rule keeps result delivery and failure propagation unambiguous.

## Evaluation and capture

The callee and every argument are evaluated at the `spawn` expression. The
call itself remains pending until `await` or scope exit. In strict ownership
code, ordinary move rules apply to captured arguments. In gradual code,
mutable containers can still alias because execution is cooperative and never
runs two Flake instructions in parallel.

The current interpreter and bytecode VM are deterministic cooperative
executors:

1. `spawn` records pending work and returns immediately.
2. `await` drives that child to completion.
3. Function return drains unawaited children.

This supplies the control-flow, type, effect, error, and lifetime contracts a
later scheduler can implement. It does not provide parallel speedup, timers,
I/O readiness, cancellation APIs, task groups, or work stealing yet.

## Backend status

| Backend | Milestone 1 behavior |
| --- | --- |
| Tree-walking interpreter | Scope-bound pending tasks, explicit/implicit join, failure propagation |
| Bytecode VM | Equivalent behavior via task values and `Spawn` / `Await` opcodes |
| Native x86-64 | Synchronous fallback: `spawn` performs the call and `await` is the identity |

The fallback means a program using these foundations can still compile and run
natively, with the same result when task behavior does not depend on scheduling.
A spawned call with visible side effects runs earlier on native, so output or
mutation order can differ. The fallback does not claim native concurrency.

## Safety boundary and future work

The language contract intentionally excludes detached tasks: work cannot
silently outlive its parent. Before true parallel execution is enabled, Flake
will need a sendability rule for captured values, explicit cancellation and
task-group policy, and a runtime scheduler. Those additions can strengthen the
implementation without changing the basic `Task[T]`, `spawn`, `await`, and
`conc` shape introduced here.
