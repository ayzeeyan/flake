# Gradual ownership

Flake's ownership system is opt-in. Ordinary functions keep script-like value
reuse; `strict fn` and `owned fn` turn on move and borrow checking where a
systems boundary needs it. The same program can introduce ownership one API at
a time instead of adopting a whole-program lifetime regime.

## Where checking applies

```flake
fn gradual(name: owned String) / io {
    print(name)
    print(name) // allowed outside a checked function
}

strict fn checked(name: owned String) / io {
    print(name)
    // print(name) // error: use of moved value `name`
}
```

- `strict fn` and `owned fn` bodies are checked.
- An `owned T` parameter or binding identifies a movable value.
- `ref T` identifies a reusable, non-assignable reference-like value.
- Copy types (`Int`, `Float`, `Bool`, and `Nil`) never move.

Annotations do not silently enable strict checking in an otherwise gradual
function. This distinction is intentional and is reflected in diagnostics.

## Moves and control flow

In a checked function, an owned value is moved when it is passed by value,
stored in a collection, returned, or used as a `let` initializer. A later use
reports the move and suggests `ref T` when reuse is intended. Assigning a new
value to a mutable owned binding makes that binding available again.

Control flow is conservative where repetition is possible:

- moving an owned value inside `loop`, `while`, or `for` is rejected because
  the body may execute again;
- after `if` / `else`, the value is considered moved only when both branches
  move it;
- `match` arms are checked independently, and matching does not itself move the
  scrutinee.

## Borrows

`&x` borrows a value and `&mut x` borrows it exclusively.

- A temporary borrow used only by the current statement ends with that
  statement: `print(&x)` followed by `print(x)` is allowed.
- A borrow stored in a binding (`let view = &x`) lasts to the end of the block
  that declared the binding.
- While a mutable borrow exists, no other borrow of the same value is allowed.
- A borrowed value cannot be moved or assigned until the borrow ends.
- `ref T` can be reused but not assigned through.

```flake
strict fn inspect(name: owned String) / io {
    print(&name) // temporary borrow
    print(name)  // move after the borrow ends
}
```

## Tasks and recoverable errors

`spawn f(args...)` captures its arguments in move positions. Passing an owned
value into child work therefore consumes it in strict code just as an ordinary
call would. Awaiting a `Task[T]` also consumes the handle in strict code; every
runtime independently enforces the single-join rule in gradual code.

Task handles cannot escape their function through a known static type. This
ties concurrency lifetime to the spawning scope even though v0.5 does not yet
have a general lifetime checker. See [concurrency.md](concurrency.md).

Postfix `?` consumes its Result-like value. `Ok(payload)` yields the payload;
`Err(error)` returns that enum value from the current function. A surrounding
`match` still checks each branch separately. See [errors.md](errors.md).

## Current boundary

v0.5 ownership is deliberately not Rust's lifetime system. Reference escapes,
aliasing through gradual `dyn` values, and all cross-task sendability cases are
not proven with full lifetime precision. True parallel scheduling must add a
captured-value sendability rule before gradual aliases can run concurrently.

Use strict ownership today to catch local moves, borrow conflicts, loop moves,
and task-handle reuse while retaining gradual code for areas that do not need
those guarantees. The [ownership examples](../examples/ownership.flk) show the
transition, and the [language tour](tour.md) supplies the surrounding syntax.
