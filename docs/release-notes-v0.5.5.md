# Flake v0.5.5 release notes

Flake v0.5.5 introduces **algebraic data types (enums)** and **rich pattern matching** across all execution backends (tree-walking Interpreter, bytecode VM, and pure-Rust x86-64 native backend), alongside significant standard library expansions and error-handling combinators.

## Key Highlights

### 1. Algebraic Data Types & Pattern Matching
- **Enum declarations**: Define enums with unit variants and payload variants (e.g. `enum Tree { Node(Tree, Tree), Leaf(Int), Empty }`).
- **Nested variant destructuring**: Match deeply nested patterns such as `Tree.Node(Tree.Leaf(a), Tree.Empty)`.
- **List pattern matching**: Destructure fixed-length list heads and elements with list patterns (e.g. `[0, y]` or `[a, b]`).
- **Exhaustiveness checking**: Static analysis guarantees all variants are handled or a wildcard arm (`_`) is provided, with diagnostics for missing variants and unreachable arms.
- **Full backend parity**: Seamless execution of enum construction and pattern matching across Interpreter, VM, and pure-Rust x86-64 native backend.

### 2. Standard Library Growth & Error Handling Ergonomics
- **`std/option.flk`**: Added `and_then_option`, `or_else_option`, `is_some_and`, `unwrap_or_else`, and nested `flatten_option`.
- **`std/result.flk`**: Added `flatten_result`, `or_else_result`, `unwrap_or_else`, `inspect_ok`, and `inspect_err`.
- **`std/list.flk`**: Added `head`, `last`, `intersperse`, `partition`, and `flat_map`.
- **`std/string.flk`**: Added `contains_str`, `count_occurrences`, and `truncate`.
- **`std/map.flk`**: Added `from_entries`, `invert_map`, `keys_list`, and `values_list`.
- **`std/math.flk`**: Added `square`, `cube`, `div_ceil`, and `in_range`.

### 3. New Examples & Multi-File Projects
- **`examples/pattern_matching.flk`**: Comprehensive standalone example demonstrating enum modeling, commands, shapes, and list patterns.
- **`examples/projects/query_engine/`**: Multi-file project demonstrating algebraic filter expressions, domain models, query execution services, and display utilities.

### 4. Zero External Dependencies
- Preserved 100% pure-Rust architecture with no LLVM, no Cranelift, and no C transpilation.
