# Structured concurrency

Flake v0.6 makes concurrency part of the language contract without pretending
that a complex external async runtime is needed. The foundation is robust and clean:
typed tasks, explicit joins, lexical task nurseries (`nursery { ... }`), cancellation primitives,
lexical ownership, deterministic failure propagation, heap-backed task states on native execution,
and a visible `conc` effect.

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

## Task nurseries (`nursery { ... }`)

Flake provides first-class structured nurseries for grouping concurrent tasks:

```flake
fn work(id: String, val: Int) -> Int { val * 2 }

fn main() / conc + io {
    let total = nursery {
        let t1 = spawn work("a", 10)
        let t2 = spawn work("b", 20)
        let r1 = await t1
        let r2 = await t2
        r1 + r2
    }
    print("total = {total}")
}
```

- **Scoped Task Lifetime**: All tasks spawned inside a `nursery` block are registered to that block's scope.
- **Automatic Drain**: When the nursery block finishes normally, any unawaited tasks are joined in spawn order before the nursery returns its result.
- **Escape Prevention**: Task handles spawned within a nursery cannot escape the block via return values or outer variables.
- **Sibling Cancellation**: If any task fails or an error occurs within the nursery, all unjoined sibling tasks are immediately cancelled.

## Task cancellation & inspection

Flake provides explicit cancellation primitives:

```flake
fn slow_job() -> Int { 100 }

fn main() / conc + io {
    let t = spawn slow_job()
    print("before: {is_cancelled(t)}")  // false
    cancel(t)
    print("after: {is_cancelled(t)}")   // true
}
```

- `cancel(task: Task[T]) -> Nil`: Cancels a pending task (requires `/ conc`).
- `is_cancelled(task: Task[T]) -> Bool`: Returns `true` if the task was cancelled.
- Attempting to `await` a cancelled task produces a `"task was cancelled"` runtime error across Interpreter, Bytecode VM, and Native x86-64 executable targets.

## Structured lifetime

Every function invocation and nursery owns a task scope. A spawned task is registered in
that scope and its handle cannot escape through a known static type. On a
successful return, the runtime joins every child that was not explicitly
awaited, in spawn order. A child error becomes a parent error. If the parent is
already failing, pending cooperative work is abandoned with the scope.

Task handles are single-use. Awaiting a joined or cancelled handle is a runtime
error (`"task was already awaited"` / `"task was cancelled"` across all backends); `strict` ownership contexts can additionally diagnose repeated moves.
This keeps result delivery and failure propagation unambiguous.

## Evaluation and capture

The callee and every argument are evaluated at the `spawn` expression. The
call itself remains pending until `await` or scope exit on the cooperative
backends. In strict ownership code, ordinary move rules apply to captured
arguments. In gradual code, mutable containers can still alias because Flake
never executes two instructions in parallel without explicit task scheduling.

Interpreter and VM use the same deterministic cooperative protocol:

1. `spawn` records pending work and returns immediately.
2. `await` drives that child to completion.
3. A successful scope exit drains unawaited children in spawn order.
4. A child failure cancels sibling tasks and reports through the parent scope.

## Backend status

| Backend | Implementation status |
| --- | --- |
| Tree-walking interpreter | Scope-bound pending tasks, nurseries, cancellation, failure propagation |
| Bytecode VM | `Op::EnterNursery`/`Op::ExitNursery`, task values, `cancel`/`is_cancelled` natives |
| Native x86-64 | Real heap Task objects with state transitions (Pending, Joined, Cancelled) and nursery unwinding |
