# Errors and Result propagation

Flake represents recoverable errors as ordinary algebraic data. There is no
hidden exception channel: a function that can fail returns an enum with one
success and one error variant.

```flake
enum Result {
    Ok(Int)
    Err(String)
}
```

## Propagating with `?`

Postfix `?` keeps the common success path compact:

```flake
fn add_two(result: Result) -> Result {
    let value = result?
    Result.Ok(value + 2)
}
```

At runtime, `Result.Ok(value)?` evaluates to `value`.
`Result.Err(error)?` immediately returns that same enum value from the
enclosing function. The operator has no effect annotation of its own.

The checker accepts any Result-like enum with exactly these two tuple variants,
in this order:

```flake
enum Name {
    Ok(SuccessType)
    Err(ErrorType)
}
```

The enclosing function must return the same enum type. If it returns another
type, Flake asks you either to change the signature or handle the error with
`match`. Both variants must carry exactly one field.

## Handling errors explicitly

Use `match` when recovery or conversion is needed:

```flake
match load() {
    Result.Ok(value) => value
    Result.Err(message) => fallback(message)
}
```

The standard `result` module provides `is_ok`, `is_err`, `unwrap_or`,
`error_or`, and panic-effectful `unwrap`. Its payload is `dyn` so the module is
useful before generic enum parameters land; user-defined Result-like enums keep
fully concrete payload types.

`?` is implemented in all execution paths. The interpreter returns the enum
value directly, the VM emits an early `Return`, and IR/native lower it to a tag
test with success and error control-flow blocks.
