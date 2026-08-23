# Flake Roadmap

**Flake v0.5.7 is complete.** Theme: **Deeper packages, more mature concurrency, and further performance work**.

There is no LLVM, Cranelift, or C transpilation.

## v0.5.7 milestones (complete)

| # | Milestone | Status |
| --- | --- | --- |
| 1 | Package system expansion | **done** |
| 2 | Concurrency maturity improvements | **done** |
| 3 | Further IR and native optimizations | **done** |
| 4 | Integration, examples and expanded testing | **done** |
| 5 | Flake v0.5.7 complete | **done** |

## What v0.5.7 milestone 1 delivers

- Public re-exports and package system expansion:
  - Added `pub import` (`pub import path as alias`) to Flake language grammar, AST, parser, and type checker.
  - Implemented transitive re-export resolution in `ModuleGraph::exported_items`: package façade entrypoints can re-export submodules and functions.
  - Enhanced `flake.toml` manifest support: `[workspace]` declaration (`members = [...]`), dependency package/version specification (`core = { path = "...", package = "..." }`), and improved syntax diagnostics.
  - 100% parity across Interpreter, Bytecode VM, and Native x86-64 executable emission for re-exported package modules and functions.

## What v0.5.7 milestone 2 delivers

- Concurrency maturity and cross-backend parity:
  - Task handles safely passed across intra-function boundaries (`fn join_both(t1: Task[Int], t2: Task[Int]) -> Int / conc`).
  - Strengthened task lifetime, error propagation, and ownership interactions (`strict owned` moved values into spawned tasks).
  - Cross-backend integration tests verifying concurrent tasks returning algebraic data types (`Result[T, E]`), multi-task join order, and single-await enforcement across Interpreter, Bytecode VM, and Native x86-64 backend.

## What v0.5.7 milestone 3 delivers

- Function inlining and native codegen optimizations:
  - Added module-level function inlining pass (`flake_ir::opt::inline_functions`): inlines non-recursive single-block leaf functions into call sites, unlocking subsequent constant folding and dead code elimination.
  - Added instruction-level local remapping (`remap_inst_locals`) preserving types and single-assignment invariants.
  - Optimized zero-immediate register loading on Native x86-64 (`mov_ri` with `imm == 0` generates compact, fast `xor reg, reg`).
  - Added optimization unit tests in `flake-ir`.

## What v0.5.7 milestone 4 delivers

- Multi-package showcase projects and end-to-end integration tests:
  - Added `service_hub` multi-package workspace example (`examples/projects/service_hub/`): multi-package project featuring public re-exports (`pub import service`), concurrent worker tasks (`spawn`/`await`), ADTs, and stdlib integration.
  - Verified 100% cross-backend parity across Interpreter, Bytecode VM, and Native x86-64 executable emission across all examples and test suites.

## What v0.5.7 milestone 5 delivers

- Documentation, release notes, and version finalization:
  - Bumped crate versions across the workspace from `0.5.6` to `0.5.7`.
  - Created complete [v0.5.7 Release Notes](docs/release-notes-v0.5.7.md).
  - Updated [README.md](README.md) and [docs/packages.md](docs/packages.md).
  - 100% clean builds, clippy checks, and test passes across the entire pure-Rust workspace.

## v0.5.6 milestones (complete)

| # | Milestone | Status |
| --- | --- | --- |
| 1 | Strengthened concurrency (conc effect improvements) | **done** |
| 2 | Package manager foundations (local packages + manifests) | **done** |
| 3 | IR and native backend optimizations | **done** |
| 4 | Integration, examples and expanded testing | **done** |
| 5 | Flake v0.5.6 complete | **done** |

## What v0.5.6 milestone 1 delivers

- Real typed heap Task objects and single-join verification on Native x86-64 backend:
  - Added `IrType::Task(Box<IrType>)`, `Inst::Spawn`, and `Inst::Await` to Flake custom IR.
  - Native x86-64 backend now allocates heap Task representations with explicit state tracking (`Pending`, `Joined`, `Running`, `Cancelled`).
  - Strict single-join runtime verification on Native x86-64: attempting to await an already-joined task raises a runtime error (`"task was already awaited"`), achieving 100% parity across Interpreter, VM, and Native backends.
  - Updated `backend_consistency.rs` and `flake-codegen` test suites verifying task errors on all three backends.
  - Updated `docs/concurrency.md` to reflect the strengthened native task runtime contract.

## What v0.5.6 milestone 2 delivers

- Package manager foundations and manifest-driven multi-package project support:
  - Package manifest format (`flake.toml`) declaring `[package]` metadata (`name`, `version`, `entry`, `authors`, `description`) and `[dependencies]` (`dep = { path = "..." }`).
  - Integrated local package dependency resolution in `flake-parser::resolve`: packages can import dependencies and nested submodules (`import dep.sub`) cleanly.
  - Extended CLI with package-aware commands:
    - `flake init [--name <name>]`: initializes a new `flake.toml` and entry file in current directory.
    - `flake new <path>`: creates a new package project structure with `flake.toml` and entry file.
    - `flake run`, `flake check`, `flake build`, `flake ir`: automatic discovery and execution of `flake.toml` root packages or targeted package directories.
  - Integration tests in `flake-parser` and `flake-cli` verifying multi-package resolution.

## What v0.5.6 milestone 3 delivers

- Comprehensive IR optimization passes and Native code generation improvements:
  - **Constant Folding & Propagation (`flake-ir::opt`)**:
    - Evaluates constant integer, float, boolean, string arithmetic and comparisons at compile time with checked overflow safety.
    - Simplifies constant condition branches to unconditional jumps.
    - Propagates single-assignment constants across basic blocks.
  - **Dead Code & Block Elimination**:
    - Prunes unreachable basic blocks via CFG reachability analysis from function entries.
    - Eliminates unused pure instructions (`LoadConst`, `Unary`, `Binary`, `Move`, `LoadFunction`).
  - **Copy Propagation**:
    - Eliminates redundant local copies (`%dest = %src`) for immutable variables.
  - **Native x86-64 Assembler Peephole Optimizations**:
    - Redundant self-move elimination (`mov reg, reg` skipped).
  - All optimizations preserve runtime error contracts and full concurrency semantics across all 3 execution engines.

## What v0.5.6 milestone 4 delivers

- Multi-package project workspace example and expanded end-to-end testing:
  - Added `examples/projects/pkg_workspace` containing multiple interoperating local packages (`core_lib` with nested submodules and `app` manifest consumer).
  - Expanded `examples.rs` integration suite verifying typechecking and runtime parity on all 3 backends (Interpreter, Bytecode VM, and Native x86-64 executable emission).
  - Validated 100% backend parity across entire consistency test matrix.

## What v0.5.6 milestone 5 delivers

- Documentation, versioning, and release finalization:
  - Bumped workspace version to `0.5.6` across all workspace crates and `Cargo.toml`.
  - Added [docs/packages.md](docs/packages.md) detailing manifest specifications, multi-package layouts, and CLI commands.
  - Added [docs/release-notes-v0.5.6.md](docs/release-notes-v0.5.6.md) documenting all new capabilities and optimizations.
  - Updated `README.md`, `docs/tour.md`, and `docs/architecture.md`.
  - Verified 100% test pass rate and clean clippy lints across the workspace.

## v0.5.5 milestones (complete)

| # | Milestone | Status |
| --- | --- | --- |
| 1 | Enums and basic pattern matching (frontend + interpreter) | **done** |
| 2 | Enums and pattern matching on the bytecode VM | **done** |
| 3 | Native x86-64 support for enums and pattern matching | **done** |
| 4 | Stdlib growth and better error-handling ergonomics | **done** |
| 5 | Examples, expanded testing and consistency | **done** |
| 6 | Flake v0.5.5 complete | **done** |

## What v0.5.5 milestone 1 delivers

- Algebraic data types and rich pattern matching across frontend and interpreter:
  - Extended `Pattern` AST with recursive subpatterns in `Pattern::Variant { ty, variant, fields: Vec<Pattern> }` and `Pattern::List { patterns: Vec<Pattern> }`.
  - Recursive pattern binding and exhaustiveness validation with diagnostics for missing enum variants and unreachable match arms.
  - Tree-walking interpreter support for nested enum variant destructuring, list pattern matching, wildcard matching, and 0-field unit variant matching.
  - Comprehensive unit tests covering nested patterns, list patterns, and diagnostic messages in `flake-interpreter` and `flake-types`.

## What v0.5.5 milestone 2 delivers

- Custom IR lowering and Bytecode VM compiler support for enums and pattern matching:
  - Full bytecode VM support for nested variant destructuring, list pattern matching, uppercase 0-arg enum variant matching, and wildcard bindings.
  - Recursive IR pattern lowering with clean control-flow graphs, type-preserving local variable allocation, and jump chaining.
  - Parity and unit tests in `flake-vm` and `flake-ir`.

## What v0.5.5 milestone 3 delivers

- Native pure-Rust x86-64 backend support for enums and pattern matching:
  - Native construction and storage of enum variant values as heap-allocated structures with tag and fields.
  - Native machine code generation for nested pattern matching, list pattern destructuring, scalar comparisons, and branch jumps.
  - End-to-end unit tests and cross-backend validation in `flake-codegen` and integration suites.

## What v0.5.5 milestone 4 delivers

- Standard library growth and error-handling ergonomics powered by enums and pattern matching:
  - `std/option.flk`: `and_then_option`, `or_else_option`, `is_some_and`, `unwrap_or_else`, and nested `flatten_option`.
  - `std/result.flk`: `flatten_result`, `or_else_result`, `unwrap_or_else`, `inspect_ok`, and `inspect_err`.
  - `std/list.flk`: `head`, `last`, `intersperse`, `partition`, and `flat_map`.
  - `std/string.flk`: `contains_str`, `count_occurrences`, and `truncate`.
  - `std/map.flk`: `from_entries`, `invert_map`, `keys_list`, and `values_list`.
  - `std/math.flk`: `square`, `cube`, `div_ceil`, and `in_range`.
  - Full cross-backend consistency test suite in `flake-cli/tests/backend_consistency.rs`.

## What v0.5.5 milestone 5 delivers

- Examples, expanded multi-file test coverage, and backend consistency:
  - Added standalone `examples/pattern_matching.flk` demonstrating shapes, commands, nested shapes, and list patterns.
  - Added multi-file project `examples/projects/query_engine/` showcasing domain AST filters, query execution, and display formatting.
  - Validated 100% output identity across tree-walking Interpreter, Bytecode VM, and pure-Rust x86-64 native backend on all examples.

## What v0.5.5 milestone 6 delivers

- Documentation, versioning, and project polish:
  - Version bump to `0.5.5` across all workspace crates.
  - Release notes published in `docs/release-notes-v0.5.5.md`.
  - Updated `README.md`, `docs/tour.md`, and `docs/examples.md`.
  - Verified 100% test passing across the entire workspace test suite.

## v0.5.2 milestones (complete)

| # | Milestone | Status |
| --- | --- | --- |
| 1 | Native backend expansion and hardening | **done** |
| 2 | Standard library growth | **done** |
| 3 | Language and builtin quality-of-life | **done** |
| 4 | Expanded examples and cross-backend testing | **done** |
| 5 | Flake v0.5.2 complete | **done** |

## What v0.5.2 milestone 1 delivers

- Native backend expansion and hardening:
  - String concatenation (`s1 + s2`) and list concatenation (`xs + ys`) via `+` operator on the native x86-64 path using `rt_concat2` and `rt_list_concat`.
  - Type-aware IR binary lowering preserving `IrType::String` and `IrType::List` for `AstBin::Add`.
  - Unit tests verifying native string and list concatenation.

## What v0.5.2 milestone 2 delivers

- Expanded and enriched standard library:
  - `std/list.flk`: `zip`, `unzip`, `take_while`, `drop_while`, `find_index`, `unique`, `count_where`, `repeat_item`, `chunk`.
  - `std/string.flk`: `starts_with_str`, `ends_with_str`, `capitalize`, `reverse_str`, `is_digit`, `is_alpha`.
  - `std/math.flk`: `abs_val`, `min_val`, `max_val`, `is_prime`, `sum_range`, `product`, `mean`.
  - `std/map.flk`: `get_or`, `merge`, `count_by`, `filter_map`, `map_values_with`.
  - `std/option.flk`: `filter_option`, `zip_option`, `expect_some`.
  - `std/result.flk`: `is_ok_and`, `is_err_and`, `expect_ok`.
  - Fix for native `UnOp::Not` instruction flag clobber and support for empty map literal `{}` syntax.
  - Complete cross-backend consistency test suite in `flake-cli/tests/backend_consistency.rs`.

## What v0.5.2 milestone 3 delivers

- Map reflection, emptiness, and membership quality-of-life builtins:
  - `entries(map)` builtin returning sorted `[[key, value], ...]` pairs across all backends.
  - `is_empty(coll)` builtin checking whether list, string, or map is empty.
  - `has_key(map, key)` builtin checking map key membership across type checker, interpreter, VM, IR, and native code.
  - 100% agreement and test coverage across Interpreter, VM, and Native x86-64.

## What v0.5.2 milestone 4 delivers

- Expanded multi-file examples and end-to-end consistency tests:
  - Multi-file analytics project in `examples/projects/analytics/` (`domain/metric.flk`, `services/aggregator.flk`, `utils/report.flk`, `main.flk`).
  - Added `analytics_project_output` test verifying CLI execution.
  - Verified 100% backend consistency and compatibility across all 25 examples for Interpreter, VM, and Native x86-64.

## What v0.5.2 milestone 5 delivers

- Documentation and release finalization:
  - Version bump to `0.5.2`.
  - Comprehensive documentation updates in `README.md`, `ROADMAP.md`, `docs/tour.md`, and `docs/examples.md`.
  - Release notes in `docs/release-notes-v0.5.2.md`.
  - 100% test pass rate across all 9 crates and backend consistency tests.

## v0.5.1 milestones (complete)

| # | Milestone | Status |
| --- | --- | --- |
| 1 | Native backend reliability improvements | **done** |
| 2 | Module system polish | **done** |
| 3 | Standard library expansion | **done** |
| 4 | Language polish and expanded testing | **done** |
| 5 | Flake v0.5.1 complete | **done** |

## What v0.5.1 milestone 1 delivers

- Native x86-64 backend reliability improvements:
  - Struct initialization layout resilience: fields in `MakeStruct` are mapped to the canonical struct definition offsets regardless of declaration order in initializers.
  - Bounds-checked list indexing in `emit_get_index` and `emit_set_index`, safely asserting on out-of-bounds access rather than reading/writing arbitrary memory.
  - Bounds validation in `rt_str_index` for string indexing and safe handling of empty strings for `first("")` / `last("")`.
  - Regression coverage in `flake-codegen` unit tests and cross-backend test suite.

## What v0.5.1 milestone 2 delivers

- Module system polish and multi-file ergonomics:
  - Robust module resolution supporting importer-relative and project-root-relative dotted modules.
  - Full struct field type propagation in IR lowering across modules and qualified names.
  - Consistent string equality comparison across dynamic and struct-field values in native code.
  - Non-trivial multi-file [pipeline project](examples/projects/pipeline/main.flk) combining domain models, transform services, format utilities, and cross-backend execution.

## What v0.5.1 milestone 3 delivers

- Practical standard library expansion across modules:
  - `std/list.flk`: `index_of`, `contains_item`, `map`, `filter`, `fold`, `any`, `all`, `flatten`, `min_item`, `max_item`.
  - `std/string.flk`: `lines`, `words`, `pad_left`, `pad_right`, `slice`, `char_at`, `to_upper`, `to_lower`, `trim_str`.
  - `std/math.flk`: `gcd`, `lcm`, `factorial`, `is_even`, `is_odd`.
  - `std/option.flk`: `is_none`, `map_option`.
  - `std/result.flk`: `map_result`, `map_err`, `and_then`.
  - 100% agreement and cross-backend test coverage across Interpreter, VM, and Native x86-64.

## What v0.5.1 milestone 4 delivers

- Map reflection and Range containment builtins:
  - `keys(map)` and `values(map)` builtins returning sorted keys and corresponding values across AST type checking, Interpreter, VM, and native x86-64 backend (`rt_map_keys` and `rt_map_values`).
  - Extended `contains(range, n)` to support forward and reverse ranges consistently across all execution backends (`rt_range_contains`).
  - Broadly expanded cross-backend regression test matrix in `flake-cli/tests/backend_consistency.rs` and `flake-codegen/src/tests.rs`.

## What v0.5.1 milestone 5 delivers

- Final documentation, version bump, and release verification:
  - Version bumped to `0.5.1` across the workspace `Cargo.toml`.
  - Updated `README.md`, `ROADMAP.md`, `docs/tour.md`, and `docs/examples.md`.
  - Added dedicated [v0.5.1 release notes](docs/release-notes-v0.5.1.md).
  - All workspace tests and example suites fully green across all three backends.

## v0.5 milestones (complete)

| # | Milestone | Status |
| --- | --- | --- |
| 1 | Structured concurrency foundations (`conc` effect) | **done** |
| 2 | Core language expansion (enums, pattern matching, errors, maps) | **done** |
| 3 | Native backend maturation | **done** |
| 4 | Strengthened module system | **done** |
| 5 | Cross-backend consistency and expanded testing | **done** |
| 6 | v0.5 polish, documentation, and examples | **done** |
| 7 | Flake v0.5 complete | **done** |

## What v0.5 milestone 1 delivers

- `spawn call(args...)` and joining syntax `await task`
- Typed, non-escaping, single-use `Task[T]` handles
- `conc` effect checking on task creation and joins, while preserving child
  effects such as `io`
- Function-owned task scopes: explicit join, implicit join before successful
  return, deterministic failure propagation, and no detached work
- Cooperative task execution in both the tree-walking interpreter and VM
- VM bytecode support through `Spawn`, `ReadyTask`, and `Await`
- Native x86-64 remains usable through a documented synchronous fallback
- Concurrency tests, [model documentation](docs/concurrency.md), and
  [example](examples/concurrency.flk)

The milestone deliberately does not add a parallel scheduler, event loop,
timers, cancellation API, or native task runtime.

## What v0.5 milestone 2 delivers

- Result-like enums gain postfix `?`: `Ok(value)` unwraps and `Err(error)`
  returns immediately from a function with the same Result-like return type
- Literal patterns for `nil`, bools, integers, floats, and strings, including
  exhaustive Bool matching
- Stronger match diagnostics for duplicate/unreachable arms, wrong enum
  patterns, missing variants, and invalid variant arity
- Enum declaration validation and checked explicit `return` values
- Maps with String, Int, or Bool keys; runtime key types no longer collide in
  the interpreter or VM
- Map membership through `contains(map, key)`, deterministic interpreter/VM
  display, and native lookup/update/membership/display for all concrete key types
- Native/IR coverage for Result propagation, scalar matching, and concrete map
  key/value types
- Expanded `std/result.flk`, dedicated [error documentation](docs/errors.md),
  and the cross-backend [data example](examples/data.flk)

## What v0.5 milestone 3 delivers

- CFG liveness analysis, interference construction, and hotness-guided greedy
  coloring onto callee-saved GPRs; non-overlapping locals reuse registers while
  simultaneous aggregate/call operands remain distinct
- Typed function-address IR and real first-class native function values,
  including local and imported functions, typed return values, and indirect
  Windows-ABI calls with stack arguments beyond the fourth parameter
- Native Float negation and deterministic decimal formatting for `print`,
  `str`, and interpolation instead of integer truncation
- Stronger map runtime behavior: concrete String/Int/Bool/Float value display
  and an explicit runtime error for a missing key
- PE32+ headers with accurate code/data sizes and virtual section sizes
- Staged executable replacement, reliable temp cleanup, and preserved native
  stdout/stderr on process failure
- `flake build` type-checks before code generation, emits only the requested
  `.exe` by default, and writes a `.s` listing only with `--emit-asm`
- Focused regression coverage for register reuse, aggregate interference,
  indirect calls, maps, floats, PE metadata, and CLI artifact behavior

## What v0.5 milestone 4 delivers

- Project-rooted dotted imports: `import services.checkout` resolves to
  `services/checkout.flk`, while simple imports retain sibling-first behavior
- Path-derived canonical module identities, so equal file stems in different
  directories stay distinct through checking, VM compilation, IR, and native code
- Private-by-default module APIs: only explicit `pub` functions, structs,
  enums, and type aliases are visible to importers, and public signatures
  cannot leak private local types
- Qualified value, type, and enum-pattern access, plus bare imported names only
  when their owner is unambiguous
- Deterministic diagnostics for duplicate aliases/paths, alias/declaration
  conflicts, ambiguous exports, missing files, and full import-cycle chains
- Per-module interpreter environments that isolate private helpers; VM and
  native functions use the same canonical module qualification
- Hierarchical [inventory](examples/projects/inventory/main.flk) and transitive
  [telemetry](examples/projects/telemetry/main.flk) projects, exercised across
  the interpreter, VM, and native backend
- Dedicated [module documentation](docs/modules.md) covering layout,
  resolution, visibility, namespaces, and current package boundaries

## What v0.5 milestone 5 delivers

- A table-driven feature matrix with exact expected output across interpreter,
  VM, and native execution for arithmetic, Floats, strings, lists, maps,
  structs, enums/patterns, Result `?`, indirect wide calls, recursion,
  short-circuiting, ranges, and pure task results
- Cross-backend failure coverage for assertions, missing map keys, child-task
  failures, and malformed builtin calls, plus cooperative scheduling and
  single-join checks for interpreter and VM
- Explicit static contracts for overloaded builtins: variadic `print`,
  optional-message `assert`, typed `abs`, homogeneous numeric `min`/`max`, and
  one- or two-argument `range`, with actionable arity/type diagnostics
- Correct native Float lowering for mixed arithmetic, remainder, comparisons,
  `abs`, `min`, and `max`, including concrete IR types and SSE operations in
  the owned x86-64 encoder; integer overflow/division failures are explicit
- Deterministic typed-key map ordering on every backend, including native map
  insertion, growth, update, and display independent of literal order
- Concrete IR list element types and consistent native display for String,
  Int, Bool, and Float lists
- Source spans attached to VM bytecode so runtime failures highlight the
  responsible expression, plus a clear CLI error for conflicting backend flags
- Dedicated [testing and consistency documentation](docs/testing.md)

## What v0.5 milestone 6 delivers

- A focused [task pipeline](examples/task_pipeline.flk) that teaches typed,
  scope-bound child work returning enum values while remaining portable across
  cooperative and synchronous execution
- A native-ready hierarchical [release gate](examples/projects/release/main.flk)
  combining explicit module APIs, public enums, exhaustive patterns, typed
  maps, Result-style `?`, and structured tasks
- One canonical example registry in the CLI integration suite; every runnable
  example is checked and compared across interpreter, VM, and native output
- A dedicated [examples guide](docs/examples.md) with learning paths, project
  layouts, native build commands, and the concurrency portability boundary
- Thoroughly reconciled language tour, architecture, ownership, concurrency,
  errors, modules, IR, code generation, and testing documentation
- README and roadmap navigation that distinguish implemented v0.5 behavior
  from post-v0.5 runtime, platform, package, lifetime, and optimization work

## What v0.5 milestone 7 delivers

- Workspace and CLI release version `0.5.0`, with all internal crates kept on
  the single workspace version
- Final interpreter, VM, native, example, diagnostic, and release-profile
  verification on the completed source tree
- A dependency and source audit confirming the pipeline remains pure Rust,
  contains no LLVM or Cranelift integration, and invokes no foreign compiler or
  transpilation stage
- Final [v0.5 release notes](docs/release-notes-v0.5.md), completed milestone
  status, and explicit post-v0.5 boundaries

## What v0.5 delivers

- First-class `conc` with typed, scope-bound, single-join `Task[T]` handles;
  interpreter and VM execute cooperative tasks while native has a documented
  synchronous fallback
- Algebraic enums, exhaustive enum/Bool matching, scalar literal patterns,
  Result-style `?`, and deterministic typed maps
- A more mature owned x86-64 pipeline with CFG-aware register allocation,
  typed indirect calls, checked integer failures, fuller Float/list/map
  behavior, and reliable PE replacement
- Comfortable hierarchical source projects through dotted imports, canonical
  module identities, private-by-default APIs, qualified types/patterns, and
  deterministic resolution diagnostics
- A 290-test workspace gate at finalization, including exact-output and shared
  failure matrices across interpreter, VM, and native plus every registered
  example
- A cohesive language tour, architecture, ownership, concurrency, module,
  error, IR, native, testing, and examples documentation set
- A 100% Rust, fully owned compiler pipeline with no LLVM, Cranelift, or
  transpilation

## v0.4 milestones (complete)

| # | Milestone | Status |
| --- | --- | --- |
| 1 | High-quality native x86-64 backend | **done** |
| 2 | Mature gradual ownership | **done** |
| 3 | Language and module expansion | **done** |
| 4 | Standard library maturity | **done** |
| 5 | Cross-backend consistency and diagnostics | **done** |
| 6 | Polish, documentation and examples | **done** |
| 7 | Flake v0.4 complete | **done** |

## What v0.4 milestone 1 delivers

- Linear-scan-style register allocation onto Windows callee-saved GPRs
- Solid Windows x64 ABI (home space, stack args, callee-saved save/restore)
- Native floats via SSE2; low-level indirect-call encoding via `call r64`
- Existing examples still match the interpreter on `flake run --native`

## What v0.4 milestone 2 delivers

- Temporary borrows (`print(&x)`) end after the statement
- Assignment is forbidden while a value is borrowed at all, not only `&mut`
- Ownership model documented in [docs/ownership.md](docs/ownership.md)

## What v0.4 milestone 3 delivers

- `enum` declarations with unit and tuple variants
- `match` expressions with qualified variant patterns, binds, `_`, and exhaustiveness checking
- Module visibility: if a file uses `pub`, only `pub` items are exported
- Interpreter, VM, and native paths all run enums and `match`
- Examples: [enum](examples/enum.flk), [visible](examples/visible.flk)

## What v0.4 milestone 4 delivers

- Prelude natives: `trim`, `upper`, `lower`, `file_exists`, `env`, `cwd`, `remove_file`
- `std/` modules: `list`, `string`, `math`, `option`, `result` (prelude + explicit imports)
- Native-path support for the new natives
- Example: [stdlib](examples/stdlib.flk)

## What v0.4 milestone 5 delivers

- Cross-backend snippet tests (interpreter, VM, native) plus every example
- `help:` notes for non-exhaustive `match`, unknown variants, missing exports, and similar names
- Private-item errors suggest marking the declaration `pub`

## What v0.4 milestone 6 delivers

- Docs (README, tour, architecture, codegen, ownership, grammar) describe v0.4
- Example: [app](examples/app.flk) — enums, stdlib, native-ready

## What v0.4 delivers

- Linear-scan register allocation and a solid Windows x64 ABI
- SSE2 floats, indirect-call encoding, enums/`match`, modules with `pub`
- Stronger gradual ownership that stays optional
- Prelude + `std/` (`list`, `string`, `math`, `option`, `result`)
- Interpreter, VM, and native agreement on examples and snippets
- Still 100% pure Rust with a fully owned compiler pipeline

## v0.3 milestones

| # | Milestone | Status |
| --- | --- | --- |
| 1 | Expanded native x86-64 backend | **done** |
| 2 | Modules and imports | **done** |
| 3 | Stronger gradual ownership | **done** |
| 4 | Standard library expansion | **done** |
| 5 | Better diagnostics and comprehensive tests | **done** |
| 6 | Polish, documentation and examples | **done** |
| 7 | Flake v0.3 complete | **done** |

## What v0.3 milestone 1 delivers

- Native path covers the majority of the language used by current examples:
  control flow, functions (including 5+ arguments), locals, arithmetic, strings,
  structs, lists, maps, ranges, interpolation, and `print`
- Built-ins lowered natively: `len`, `push`, `pop`, `join`, `split`, `abs`,
  `min`, `max`, `range`, `str`, `int`, `type_of`, `assert`, `read_file`
- `flake run --native` matches the interpreter on every current example
- Still no LLVM, Cranelift, or C transpilation

## What v0.3 milestone 2 delivers

- `import math` loads sibling `math.flk`; `import math as m` binds a namespace
- Qualified calls (`math.add`) and unambiguous bare names both work
- Type checker, interpreter, VM, and native path all understand modules
- Example: [examples/modules.flk](examples/modules.flk)

## What v0.3 milestone 3 delivers

- Clearer ownership diagnostics with `help:` notes
- Borrows last until the end of the current block (then the value may be used again)
- Moving an `owned` value inside a loop is rejected
- If/else: a value is moved after the `if` only if both branches move it
- Unannotated code is unchanged

## What v0.3 milestone 4 delivers

- Prelude natives: `write_file`, `contains`, `starts_with`, `ends_with`, `first`, `last`
- Flake modules under `std/` (`list`, `string`), found by walking up from the importer
- Example: [examples/stdlib.flk](examples/stdlib.flk)

## What v0.3 milestone 5 delivers

- `help:` notes in ownership errors are shown as miette help text
- Missing `import` names the module and search path
- Tests cover write/read file, stdlib natives, missing modules, and
  interpreter / VM / native agreement on every example

## What v0.3 milestone 6 delivers

- README, tour, architecture, and codegen docs describe native code, modules,
  ownership, and the stdlib
- Examples: [modules](examples/modules.flk), [stdlib](examples/stdlib.flk),
  [borrow](examples/borrow.flk)

## v0.2 milestones

| # | Milestone | Status |
| --- | --- | --- |
| 1 | Bytecode VM feature parity | **done** |
| 2 | Custom intermediate representation | **done** |
| 3 | Native x86-64 codegen foundation | **done** |
| 4 | Expanded native code generation | **done** |
| 5 | Improved gradual ownership | **done** |
| 6 | Polish, tests, documentation and examples | **done** |
| 7 | Flake v0.2 complete | **done** |

## What v0.2 delivers

- `flake run --vm` runs every example (matches the tree-walker)
- Custom CFG IR (`flake ir file.flk`)
- Native path: `flake run --native` / `flake build` → PE32+ x86-64
  (functions, integers, strings, control flow, structs, lists, `print`)
- Stronger gradual ownership: borrows (`&` / `&mut`) in `strict` contexts,
  exclusive mutable borrows, no move-while-borrowed; ordinary code still
  needs no annotations
- Docs: [IR](docs/ir.md), [codegen](docs/codegen.md), updated tour and architecture

## v0.1 (complete)

Lexer, parser, AST, gradual types/effects/ownership, interpreter, VM
foundation, REPL. See git history milestones 0–10.

## Direction after v0.5

The next work should extend the foundations without weakening the current
cross-backend contract:

1. **Concurrency runtime:** task groups and cancellation semantics, captured
   value sendability, a scheduler, I/O readiness, and eventually a native task
   runtime. Detached work remains intentionally excluded.
2. **Targets and artifacts:** aarch64 plus System V x86-64/ELF support while
   retaining the fully owned code-generation pipeline.
3. **Packages:** manifests, public re-exports, dependency aliases, versioned
   resolution, a registry, and lockfiles; no package-manager behavior is
   implied by v0.5 modules.
4. **Ownership:** stronger reference escape and lifetime analysis, building on
   the existing opt-in `strict` model rather than forcing ceremony everywhere.
5. **Optimization:** inlining and other profile-guided or CFG-level passes
   beyond the current liveness/interference allocator.
6. **Long horizon:** self-hosting after the language, package model, and
   multi-target native pipeline are stable enough to support it.
