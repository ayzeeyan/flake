# Structured concurrency

Flake v0.5.6 makes concurrency part of the language contract without pretending
that a complex external async runtime is needed. The foundation is robust and clean:
typed tasks, explicit joins, lexical ownership, deterministic failure propagation,
heap-backed task states on native execution, and a visible `conc` effect.

## Surface and effects

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
- `await task` joins that task and returns its `T` result. It is unrelated to
  the `join(list, separator)` string helper.
- Both operations perform the `conc` effect.
- Effects of the child call remain visible. Spawning an `/ io` function needs
  both `conc` and `io` in the parent.
- There is no special `async fn` form. An ordinary function call
  becomes child work when it appears as the operand of `spawn`.

The checker rejects `spawn` on a non-call expression, `await` on a non-task,
missing `conc`, and a statically known task escape.

## Structured lifetime

Every function invocation owns a task scope. A spawned task is registered in
that scope and its handle cannot escape through a known static type. On a
successful return, the runtime joins every child that was not explicitly
awaited, in spawn order. A child error becomes a parent error. If the parent is
already failing, pending cooperative work is abandoned with the scope.

Task handles are single-use. Awaiting a joined or cancelled handle is a runtime
error (`"task was already awaited"` across all backends); `strict` ownership contexts can additionally diagnose repeated moves.
This keeps result delivery and failure propagation unambiguous.

```flake
fn answer() -> Int { 42 }

fn one_join() -> Int / conc {
    let task = spawn answer()
    let value = await task
    // await task  // rejected at runtime on all backends; also a move error in strict code
    value
}
```

## Evaluation and capture

The callee and every argument are evaluated at the `spawn` expression. The
call itself remains pending until `await` or scope exit on the cooperative
backends. In strict ownership code, ordinary move rules apply to captured
arguments. In gradual code, mutable containers can still alias because Flake
never executes two instructions in parallel without explicit task scheduling.

Interpreter and VM use the same deterministic cooperative protocol:

1. `spawn` records pending work and returns immediately.
2. `await` drives that child to completion.
3. A successful function return drains unawaited children in spawn order.
4. A child failure is reported through the parent scope.

This supplies the control-flow, type, effect, error, and lifetime contracts a
later scheduler must preserve. It does not provide parallel speedup, timers,
I/O readiness, cancellation APIs, task groups, or work stealing.

## Portable task programs

For output that agrees across all backends, keep child work free of
scheduling-visible I/O or shared mutation and print after joining:

```flake
fn square(n: Int) -> Int { n * n }

fn main() / conc + io {
    let left = spawn square(6)
    let right = spawn square(7)
    print(await left + await right)
}
```

The runnable [task pipeline](../examples/task_pipeline.flk) and multi-file
[release gate](../examples/projects/release/main.flk) follow this rule. They
compile unchanged on the native backend and have exact output parity tests.

## Backend status

| Backend | Implementation status |
| --- | --- |
| Tree-walking interpreter | Scope-bound pending tasks, explicit/implicit join, failure propagation |
| Bytecode VM | Equivalent behavior through task values and `Spawn` / `Await` bytecode |
| Native x86-64 | Real heap Task objects with state tracking and strict single-join runtime verification |

In v0.5.6, all three execution engines enforce the single-join runtime contract:
attempting to join an already-awaited task raises a runtime error on Interpreter, VM,
and Native x86-64. Task handles are first-class heap values that can be passed across
functions and checked for state.

## Safety boundary and future work

Detached tasks are intentionally excluded: work cannot silently outlive its
parent. Before true parallel multi-threaded execution is enabled, Flake needs a sendability
rule for captured values, explicit cancellation and task-group policy, and a
multi-threaded runtime scheduler. Native scheduling must then preserve scope exit, child
failure, and single-result rules rather than inventing a second concurrency
model.
