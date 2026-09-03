# Flake v1.0 Frozen Stable Subset Contract

This document specifies the **frozen language contract** for Flake v1.0.0. Features defined herein are normative, stable, and covered by the 1.0 backward compatibility guarantee.

---

## 1. The v1.0 Backward Compatibility Promise

> [!IMPORTANT]
> **The 1.0 Compatibility Guarantee**:
> Any program conforming to this frozen stable subset that `flake check` accepts in Flake 1.0.0 is guaranteed to remain accepted, compile, and preserve identical observable runtime semantics across all subsequent 1.x releases (`1.1.0`, `1.2.0`, etc.).
>
> Breaking syntax changes, removal of supported standard library APIs, or alterations to ownership and effect rules will never occur in the 1.x series and are reserved exclusively for a future 2.0. Soundness fixes to close unintentional type or safety holes are the sole exception and will be documented explicitly.

---

## 2. In-Scope: The Frozen Stable Surface

### A. Program Structure & Declarations
- **Top-Level Items**:
  - Functions: `fn name[T](params...) -> Ret / effects { body }`
  - Structs: `struct Name[T] { field: Type, ... }`
  - Enums: `enum Name[T] { Variant, Variant(Type), ... }`
  - Type Aliases: `type Alias[T] = Type`
  - Traits: `trait Name[T] { fn method(self, ...) -> Ret }`
  - Trait Implementations: `impl Name for Type { fn method(self, ...) -> Ret { ... } }`
  - Constant Items: `const NAME: T = <expr>`
  - Constant Functions: `const fn name(params...) -> Ret { ... }`
  - Imports: `import module`, `import module as alias`, `pub import module`

### B. Types & Generics
- **Primitive Scalars**: `Int` (64-bit signed), `Float` (64-bit IEEE 754), `String` (UTF-8 immutable), `Bool` (`true` / `false`), `Nil` (`()`).
- **Composite Collections**: `List[T]`, `Map[K, V]` (where `K` implements `Eq + Hash`).
- **Container Types**: `Option[T]` (`Some(v)` / `None`), `Result[T, E]` (`Ok(v)` / `Err(e)`), `Task[T]`.
- **Gradual Typing Escape Hatch**: `dyn` for dynamic interop and gradual typing boundaries.
- **Parametric Polymorphism**: Generic type parameters on functions, structs, enums, and traits.
- **Trait Bounds & Standard Traits**:
  - Bound syntax: `fn sort[T: Ord](items: List[T]) -> List[T]`
  - Standard traits: `Eq`, `Ord` (requires `Eq`), `Hash`.
  - Concrete and bound trait method dispatch.

### C. First-Class Algebraic Effect System
- **Declared Effects**: Functions declare side-effect sets after `/`:
  - `io`: Terminal, filesystem, and process I/O.
  - `alloc`: Heap memory allocation.
  - `conc`: Structured concurrency operations (`spawn`, `await`, `nursery`).
  - `panic`: Unrecoverable errors and assertion failures.
  - `pure`: Explicitly effect-free computation.
- **Effect Inference**: Unannotated helper functions infer required effect sets from their bodies and propagate them to callers.
- **Effect Boundaries**: Pure functions cannot perform side-effects or call unannotated functions with undeclared side-effects.

### D. Gradual Ownership & Borrowing
- **Linear Ownership**: `strict` and `owned` function modifiers track value moves and prevent use-after-move.
- **Borrowing**: Shared references `&T` and exclusive mutable references `&mut T`.
- **Reference Lifetimes**: Rejects escaping local stack references (`return &x`).
- **Concurrency Sendability**: Task `spawn` boundaries reject borrowed references (`&T`, `&mut T`), requiring owned values.

### E. Structured Concurrency
- **Task Spawning**: `spawn callee(...)` requires the `conc` effect and returns a scope-bound `Task[T]`.
- **Task Awaiting**: `await task` yields `T` (or propagates failure). Tasks may only be awaited once.
- **Structured Nurseries**: `nursery { ... }` blocks guarantee all spawned child tasks complete before scope exit.
- **Cancellation & Inspection**: `cancel(task)`, `is_completed(task)`, `is_cancelled(task)`, `task_status(task)`.

### F. CTFE Lite (Compile-Time Function Evaluation)
- **Constant Items**: Top-level `const NAME: T = <expr>` evaluated during type check and IR lowering.
- **Pure Constant Functions**: `const fn` evaluated at compile time without I/O or concurrency.
- **Constant Folding**: Integer, float, and boolean arithmetic, comparisons, logic, `if`/`else` branching, string concatenation, and string interpolation.
- **Safety Fuel & Recursion Limits**:
  - `CTFE_FUEL = 10_000` instructions.
  - `MAX_CALL_DEPTH = 256` frames.
  - Guaranteed termination on all targets.

### G. Standard Library Modules
- **`std/fs`**: `read_to_string`, `write_string`, `read_dir`, `walk`, `read_lines`, `write_lines`, `append_string`, `exists`, `remove`, `file_size`, `is_directory`, `is_regular_file`.
- **`std/path`**: `join_path`, `file_name`, `parent`, `extension`, `normalize`, `is_absolute`.
- **`std/process`**: `program_args`, `current_dir`, `env_var`, `run`, `exit`.
- **`std/list`**: `sort_items`, `find_eq`, `contains_eq`, `max_ord`, `min_ord`.
- **`std/string`**, **`std/option`**, **`std/result`**, **`std/math`**, **`std/map`**, **`std/bytes`**, **`std/channel`**.

### H. Native Execution & Target Matrix
- **`x86_64-windows`**: PE32+ native executable format with Win32 systems runtime (`KERNEL32.dll`).
- **`x86_64-linux`**: ELF64 native executable format with direct Linux syscall runtime (`runtime_linux.rs`, zero libc dependency).
- **Backend Consistency**: Identical evaluation across Tree-walking Interpreter, Bytecode VM, and Native x86-64 machine code.

---

## 3. Out-of-Scope: Non-Goals & Boundaries for 1.x

The following features are **explicitly excluded** from Flake v1.0 and will not be added to 1.x:

1. **Macro Systems**:
   - Macro expansion, procedural macros, token tree inspection, and AST quasiquoting are out of scope. Flake relies on clear syntax, generics, and traits.
2. **Compile-Time I/O**:
   - CTFE cannot read files, access network sockets, execute shell commands, or perform nondeterministic system calls.
3. **Flake Codegen in Flake**:
   - The native assembler, PE/ELF writer, and register allocator remain implemented in the pure Rust host compiler (`flake-codegen`). Flake v1.0 does not attempt to rewrite the code generator in Flake.
4. **Authoritative Checker Replacement**:
   - The Rust host checker (`flake-types`) is the compiler of record. The self-hosted checker (`selfhost/frontend/`) is a validation artifact and bootstrap proof.
5. **AArch64 Systems Completeness**:
   - AArch64 Linux ELF emission is a partial target. Core instruction encodings exist, but systems APIs are stubs. Full AArch64 systems runtime is documented as future 1.x work.
6. **Production Async Event Loop**:
   - Structured concurrency uses OS threads and cooperative join runtimes. Work-stealing async runtimes or epoll/kqueue event loops are out of scope for the core language.

---

## 4. Automated Enforcement

The stable subset contract is enforced on every commit by automated AST visitors:
- **`flake-cli/tests/selfhost_subset_lock.rs`**: Audits all 11 files under `selfhost/frontend/` and all 62 files under `examples/` to guarantee that no disallowed builtins, unapproved effects, or macro-like constructs exist.
- **`flake-cli/tests/bootstrap.rs`**: Verifies that the native bootstrap loop completes with 100% bitwise and behavioral identity.
