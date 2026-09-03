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
`error_or`, and panic-effectful `unwrap`. User-defined Result-like enums
and generic `Result[T, E]` enums keep fully concrete payload types.

The multi-file [release gate](../examples/projects/release/main.flk) exposes a
public Result-like enum from a service module, propagates a failed check with
`?`, and handles both variants at the application boundary.

`?` is implemented in all execution paths. The interpreter returns the enum
value directly, the VM emits an early `Return`, and IR/native lower it to a tag
test with success and error control-flow blocks.

## Recoverable values and runtime failures

Result-like enums model failures the caller is expected to handle. Assertions,
integer overflow, division by zero, missing map keys, invalid task joins, and
failed panic-effectful helpers remain runtime failures. Functions that can
trigger an assertion or `unwrap` expose the `panic` effect; `contains(map,
key)` is the non-failing way to test map presence before indexing.

VM runtime instructions retain source spans, so CLI diagnostics highlight the
failing expression. Native failures preserve the child process's stdout and
stderr. Shared semantic markers for these boundaries are exercised by the
[backend consistency suite](testing.md).
