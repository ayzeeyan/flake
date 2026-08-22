# Flake v0.5.2 release notes

Flake v0.5.2 delivers further stability, richer standard library growth, practical usability improvements, and expanded cross-backend verification while maintaining pure-Rust implementations across all subsystems.

## Native backend expansion and hardening

- **String and list concatenation**: Native x86-64 code generator now directly supports string concatenation (`s1 + s2`) and list concatenation (`xs + ys`) via the `+` operator, using `rt_concat2` and `rt_list_concat`.
- **Instruction flag clobber fix**: Fixed `UnOp::Not` instruction generation in the native backend where intermediate register zeroing clobbered CPU zero flags, restoring accurate boolean inversion on the native path.
- **Nested collection display**: Native runtime routines `rt_display_list` and `rt_display_map` now support recursive display of nested lists and maps with concrete element types.
- **Pointer-key detection**: Map access and membership routines (`rt_map_get`, `rt_map_set`, `rt_map_has`, `rt_display_map`) auto-detect heap string pointers vs integer scalar keys in dynamic contexts.

## Standard library expansion

- **`std/list.flk`**: Added `zip`, `unzip`, `take_while`, `drop_while`, `find_index`, `unique`, `count_where`, `repeat_item`, and `chunk`.
- **`std/string.flk`**: Added `starts_with_str`, `ends_with_str`, `capitalize`, `reverse_str`, `is_digit`, and `is_alpha`.
- **`std/math.flk`**: Added `abs_val`, `min_val`, `max_val`, `is_prime`, `sum_range`, `product`, and `mean`.
- **`std/map.flk`**: New standard library module with `get_or`, `merge`, `count_by`, `filter_map`, and `map_values_with`.
- **`std/option.flk`**: Added `filter_option`, `zip_option`, and `expect_some`.
- **`std/result.flk`**: Added `is_ok_and`, `is_err_and`, and `expect_ok`.

## Language and builtin quality-of-life

- **`entries(map)`**: Builtin function returning a deterministic list of sorted 2-element pairs `[[key, value], ...]` across Interpreter, Bytecode VM, and Native x86-64 backend (`rt_map_entries`).
- **`is_empty(coll)`**: Builtin function checking emptiness for lists, strings, and maps across all backends (`rt_is_empty`).
- **`has_key(map, key)`**: Map key membership builtin checking key presence without missing-key runtime errors.
- **Empty map literal `{}`**: Parser support for empty map literals `{}` in expression position.

## Examples and testing

- **Analytics project (`examples/projects/analytics/`)**: Added multi-file example project demonstrating domain models (`MetricSample`), aggregation services (`AggregateResult`), report utilities, and cross-backend execution.
- **Cross-backend consistency**: Complete test suite ensuring 100% agreement across all 25 examples and standard library expansions between the Interpreter, Bytecode VM, and Native x86-64 compiler.

## Pure Rust foundation

Flake continues to be built with zero external code generation dependencies:
- Lexer, parser, type checker, interpreter, bytecode VM, custom IR, and machine code emitter are 100% pure Rust (no LLVM, Cranelift, or C transpilation).
