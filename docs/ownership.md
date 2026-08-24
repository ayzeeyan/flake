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

## Borrows and Structural Borrow Checking

`&x` borrows a value and `&mut x` borrows it exclusively.

- A temporary borrow used only by the current statement ends with that
  statement: `print(&x)` followed by `print(x)` is allowed.
- A borrow stored in a binding (`let view = &x`) lasts to the end of the block
  that declared the binding.
- While a mutable borrow exists, no other borrow of the same value is allowed.
- A borrowed value cannot be moved or assigned until the borrow ends.
- `ref T` can be reused but not assigned through.

### Structural borrow conflict detection

Borrowing an interior field or element (`&p.field` or `&list[idx]`) tracks the root container binding. While an interior borrow is active:
- The parent container cannot be moved (`cannot move \`p\` while it is borrowed`).
- Conflicting mutable borrows to the same container are rejected.
- Mutating fields while a structural borrow is active is prohibited.

### Match arm pattern binding ownership

In `strict` and `owned` functions, pattern bindings (e.g. `Option.Some(val)`, `Point { x, y }`, `(a, b)`) introduce owned or reference bindings into the arm scope. Moving an arm-bound value multiple times is caught and rejected by ownership analysis.

```flake
strict fn inspect(name: owned String) / io {
    print(&name) // temporary borrow
    print(name)  // move after the borrow ends
}
```

## Tasks, sendability, and recoverable errors

`spawn f(args...)` captures its arguments in move positions. Passing an owned
value into child work therefore consumes it in strict code just as an ordinary
call would. In addition:
- Borrowed references (`&x`, `&mut x`, `ref T`) are forbidden from escaping across `spawn` boundaries.
- Task handles cannot escape their nursery or spawning function.
- Awaiting a `Task[T]` consumes the handle in strict code; every runtime enforces the single-join contract in gradual code.

Postfix `?` consumes its Result-like value. `Ok(payload)` yields the payload;
`Err(error)` returns that enum value from the current function. A surrounding
`match` still checks each branch separately. See [errors.md](errors.md).

## Summary

Flake's gradual ownership model gives developers total control: write fluid, expressive code where appropriate, and opt into strict compile-time borrow checking and move tracking when building high-reliability systems components. The [ownership examples](../examples/ownership.flk) demonstrate these guarantees in action.
