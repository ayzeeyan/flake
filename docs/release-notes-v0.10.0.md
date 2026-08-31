# Flake v0.10.0 Release Notes: Trait methods and usable bounds

Flake v0.10.0 is Phase 1 of 6 on the road to v1.0. It delivers trait methods,
usable bounds, and full cross-backend dispatch across the Tree-walking Interpreter,
Bytecode VM, and pure Rust Native x86-64 executable backend. It also finishes
native `process.run` with Win32 pipe capture, providing the full foundation for
Phase 2 (self-hosted frontend in v0.11).

Workspace version: **0.10.0**.

---

## 1. Trait methods and declarations

Traits can now declare method signatures with `self` receivers, parameters, return types, and effect sets:

```flake
trait Show {
    fn show(self) -> String
}

impl Show for Int {
    fn show(self) -> String {
        str(self)
    }
}

impl Show for String {
    fn show(self) -> String {
        self
    }
}
```

- Typechecker validates that implementations provide all required methods with matching arities, names, and return types.
- Missing methods, unexpected methods, or duplicate methods in trait declarations and impl blocks are rejected with rich diagnostics.

## 2. Usable trait bounds

Generic functions and types can constrain type parameters with trait bounds and directly invoke trait methods on receiver expressions:

```flake
fn display[T: Show](value: T) -> String {
    value.show()
}

fn main() / io {
    print(display(42))
    print(display("crystallized"))
}
```

- Trait method invocation is checked statically: calling a method on `T` requires a corresponding trait bound `T: Trait`.
- Diagnostics point clearly to missing trait bounds or unimplemented traits.

## 3. Cross-backend dispatch consistency

Trait method dispatch behaves identically across all three execution targets:

1. **Tree-walking Interpreter (`flake run`)**:
   - Trait implementations are registered in the runtime environment.
   - Dynamic method resolution on receiver types (`Int`, `String`, `Struct`, etc.).
2. **Bytecode VM (`flake run --vm`)**:
   - Impl methods compiled into qualified bytecode globals (`{Type}_{method}`).
   - Dedicated `Op::CallMethod` and `Op::SpawnMethod` instructions.
3. **Pure Rust Native x86-64 Backend (`flake run --native`)**:
   - Monomorphization pass instantiates bounded generic templates.
   - Direct static calls (`Callee::Static("{Type}_{method}")`) with zero dynamic vtable overhead.

## 4. Native `process.run` (stdout capture & exit code)

Native x86-64 code execution now includes real process spawning and stdout capture via Win32 pipe redirection:

- Creates anonymous pipes with child inheritance.
- Spawns child commands via `CreateProcessA` with redirected standard handles.
- Captures output via `ReadFile` and retrieves status via `GetExitCodeProcess`.
- Fully conforms with `std/process.flk` `ProcessOutput { stdout, stderr, exit_code }` on all backends.

## 5. Showcase and tests

- Updated [examples/traits.flk](../examples/traits.flk) demonstrating trait declarations, method implementations, bounds, and dispatch.
- New [examples/ast_show.flk](../examples/ast_show.flk) dogfooding trait methods for AST node formatting in anticipation of Phase 2.
- Updated [docs/stable-subset.md](stable-subset.md) detailing the stabilized language surface.
- 36 integration tests and 29 cross-backend consistency test suites passing cleanly.

## 6. What's next: Phase 2 (v0.11)

Phase 1 (Trait methods + usable bounds) is complete. The next milestone is Phase 2 of 6:
**Self-hosted frontend (lexer + parser in Flake)** in `v0.11.0`.