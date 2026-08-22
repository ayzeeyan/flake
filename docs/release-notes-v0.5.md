# Flake v0.5 release notes

Flake v0.5 turns the project from a broad compiler prototype into a more usable
systems-language foundation. It adds explicit structured tasks, richer data and
error handling, stronger multi-file organization, and materially tighter
agreement between the interpreter, bytecode VM, and native x86-64 path.

## Language

- `spawn call(args...)` and `await task` operate on typed, scope-bound
  `Task[T]` handles under the `conc` effect.
- Enums support unit and tuple variants, exhaustive `match`, qualified variant
  patterns, and literal patterns for `nil`, Bool, Int, Float, and String.
- Result-like enums can use postfix `?` for typed early propagation.
- `Map[K, V]` supports String, Int, or Bool keys with deterministic typed-key
  display, membership, lookup, assignment, and native runtime support.
- Overloaded prelude functions now have explicit checker contracts and clearer
  arity/type diagnostics.

## Execution paths

- The interpreter and VM implement deterministic cooperative child tasks,
  implicit scope exit joins, single-result handles, and child failure
  propagation.
- Native compilation supports pure task programs through a documented
  synchronous fallback; a scheduler and native task runtime remain future work.
- Native Float arithmetic/comparison/helpers, concrete list display, ordered
  maps, indirect calls, integer failure behavior, register allocation, and PE
  production are substantially more complete.
- VM bytecode retains source spans for runtime diagnostics.

## Projects and quality

- Dotted project-rooted imports, canonical module identities, explicit `pub`
  APIs, qualified values/types/patterns, and isolated private helpers make
  hierarchical source projects practical.
- Exact-output success cases and shared failure cases compare all three
  execution paths. Every registered example is checked and run on interpreter,
  VM, and native.
- The new [task pipeline](../examples/task_pipeline.flk) and
  [release gate](../examples/projects/release/main.flk) provide focused and
  multi-file demonstrations of the v0.5 surface.

## Compatibility boundary

Flake v0.5 emits Windows x86-64 PE executables. It does not yet provide a
parallel async scheduler, cancellation API, full lifetime checker, package
manager, non-Windows native target, or self-hosting compiler. The roadmap keeps
those directions explicit.

The implementation remains pure Rust and owns its lexer, parser, checker,
interpreter, bytecode VM, IR, machine-code encoder, runtime, and PE writer. It
uses no LLVM, Cranelift, foreign compiler, or transpilation stage.
