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

Flake provides explicit cancellation and inspection primitives:

```flake
fn slow_job() -> Int { 100 }

fn main() / conc + io {
    let t = spawn slow_job()
    print("status: {task_status(t)}")    // "pending"
    print("cancelled: {is_cancelled(t)}") // false
    print("completed: {is_completed(t)}") // false

    cancel(t)
    print("status: {task_status(t)}")    // "cancelled"
    print("cancelled: {is_cancelled(t)}") // true
}
```

- `cancel(task: Task[T]) -> Nil`: Cancels a pending task (requires `/ conc`).
- `is_cancelled(task: Task[T]) -> Bool`: Returns `true` if the task was cancelled.
- `is_completed(task: Task[T]) -> Bool`: Returns `true` if the task is completed/joined.
- `task_status(task: Task[T]) -> String`: Returns `"pending"`, `"running"`, `"completed"`, `"joined"`, or `"cancelled"`.
- Attempting to `await` a cancelled task produces a `"task was cancelled"` runtime error across Interpreter, Bytecode VM, and Native executable targets.

## Sendability & Capture Rules

When spawning child work with `spawn f(args...)`:
- **Reference escape prevention**: Arguments cannot be borrowed references (`&x`, `&mut x`, or `ref T`), preventing dangling references across task boundaries.
- **Strict ownership consumption**: In `strict` functions, captured owned arguments are consumed (moved) into the spawned task.
- **Single-join enforcement**: Task handles are single-use across all backends.

## Backend status

| Backend | Implementation status |
| --- | --- |
| Tree-walking interpreter | Scope-bound tasks, nurseries, cancellation, lifecycle state queries, failure propagation |
| Bytecode VM | `Op::EnterNursery`/`Op::ExitNursery`, task values, `cancel`/`is_cancelled`/`is_completed`/`task_status` natives |
| Native (PE/ELF) | Heap-allocated Task objects with full state transitions (`Pending`, `Running`, `Completed`, `Joined`, `Cancelled`) |
