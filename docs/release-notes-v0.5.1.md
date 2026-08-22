# Flake v0.5.1 release notes

Flake v0.5.1 delivers solid incremental improvements, enhanced native backend
reliability, module system polish, standard library expansion, and high-value
language quality-of-life additions while maintaining pure-Rust implementations
across all subsystems.

## Native backend reliability

- **Struct layout resilience**: `MakeStruct` in the native x86-64 backend aligns
  field stores to canonical struct definition offsets, ensuring out-of-order field
  initializers in source code always initialize fields at correct memory offsets.
- **Bounds-checked indexing**: List index lookups and mutations (`emit_get_index`
  and `emit_set_index`) safely validate normalized indices against list length,
  cleanly asserting on out-of-bounds access.
- **Safe string operations**: `rt_str_index` validates index boundaries, handling
  `first("")` and `last("")` safely without invalid memory reads.
- **Dynamic string comparison**: Native binary comparisons with dynamic/untyped
  operands use `rt_streq` when either operand is a string.

## Module system polish

- **Flexible module resolution**: `find_module` supports both importer-relative
  and project-root-relative dotted modules, enabling deep hierarchical structures
  with clean imports.
- **Struct field type propagation**: IR lowering preserves concrete struct field
  types across module boundaries and qualified namespaces.
- **New multi-file example**: Added [`projects/pipeline/main.flk`](../examples/projects/pipeline/main.flk),
  a comprehensive batch data transformation pipeline verified across Interpreter,
  Bytecode VM, and Native x86-64.

## Standard library expansion

- **`std/list.flk`**: Added `index_of`, `contains_item`, `map`, `filter`, `fold`,
  `any`, `all`, `flatten`, `min_item`, and `max_item`.
- **`std/string.flk`**: Added `lines`, `words`, `pad_left`, `pad_right`, `slice`,
  `char_at`, `to_upper`, `to_lower`, and `trim_str`.
- **`std/math.flk`**: Added `gcd`, `lcm`, `factorial`, `is_even`, and `is_odd`.
- **`std/option.flk`**: Added `is_none` and `map_option`.
- **`std/result.flk`**: Added `map_result`, `map_err`, and `and_then`.

## Language improvements

- **`keys(map)` and `values(map)`**: Built-in functions returning deterministic
  sorted keys and corresponding values across type checking, Interpreter, VM, and
  Native x86-64 backend (`rt_map_keys` and `rt_map_values`).
- **`contains(range, n)`**: `contains` now supports forward and reverse integer
  ranges across all three backends (`rt_range_contains`).

## Pure Rust foundation

Flake remains 100% pure Rust:
- Lexer, parser, type checker, interpreter, bytecode VM, custom IR, and x86-64
  machine code generator are implemented without external code-gen dependencies
  (no LLVM, Cranelift, or C transpilation).
