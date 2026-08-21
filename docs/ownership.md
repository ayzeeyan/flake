# Gradual ownership

Flake's ownership system is **opt-in**. Ordinary functions may reuse values
freely. `strict` and `owned` functions turn on move and borrow checking.

## When rules apply

- `strict fn` and `owned fn` are checked.
- Unannotated functions are not. `fn reuse(name: owned String)` may print
  `name` twice.

## Moves

In a checked function, an `owned` value is **moved** on use in a move
position (passed to a call, stored in a list, returned, used as a `let`
initializer). A later use is an error:

```
use of moved value `x`
help: in `strict` functions, `owned` values are moved on use; take `ref T` to reuse
```

Copy types (`Int`, `Float`, `Bool`, `Nil`) never move. `ref T` may be used
many times and cannot be assigned.

Re-assignment (`x = "b"`) makes an `owned` binding available again.

Moving inside a `loop` / `while` / `for` is rejected: the body may run more
than once. After `if` / `else`, a value is moved only if **both** branches
moved it.

## Borrows

`&x` and `&mut x` borrow without moving.

- A borrow used only in the current statement (a call argument) ends when
  that statement finishes: `print(&x)` then `print(x)` is allowed.
- A borrow stored in a binding (`let r = &x`) lasts until the end of the
  block that declared `r`.
- `&mut` is exclusive: no other borrow of `x` while it is mutably borrowed.
- You cannot move or assign `x` while it is borrowed.

`match` arms are checked independently; the scrutinee is not moved.

There is no full lifetime checker. Returning a reference to a local is not
tracked as thoroughly as in Rust. See also [the tour](tour.md).
