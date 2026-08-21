# Native x86-64 backend

Flake's native backend is written entirely in-tree. There is no LLVM, Cranelift,
or C transpilation.

```
source → AST → IR → x86-64 machine code → PE32+ executable
```

On Windows the compiler emits a real `.exe` (and a `.s` sidecar for inspection).
`flake run --native` compiles to a temp executable and runs it.

```bash
flake build examples/hello.flk -o hello.exe
flake run --native examples/hello.flk
```

## Calling convention

Generated functions use the **Windows x64 ABI**:

- integer/pointer arguments in `rcx`, `rdx`, `r8`, `r9`
- return value in `rax`
- 32 bytes of home space reserved in every frame

## Runtime

A small hand-written runtime is linked into every image:

| Symbol | Role |
| --- | --- |
| `rt_print_cstr` | `WriteFile` a C string |
| `rt_print_i64` | decimal print |
| `rt_concat2` | heap-concatenate two C strings |
| `rt_itoa` | integer → heap C string |
| `rt_alloc` | `HeapAlloc` |
| `rt_join` | join a list of strings |
| `rt_streq` / `rt_split` | string equality and split |
| `rt_list_*` / `rt_map_*` | growable lists and linear maps |
| `rt_display_*` | list / map / range interpolation |
| `rt_read_file` | `CreateFileA` + `ReadFile` |

Imports are resolved from `KERNEL32.dll` via a standard PE import table.

## What compiles natively (v0.3)

Integers, bools, strings, `if`/`while`/`for` (lists and ranges), functions
(including more than four arguments), structs, lists, maps, interpolation,
`print`, and the built-in helpers (`len`, `push`, `pop`, `join`, `split`,
`abs`, `min`, `max`, `range`, `str`, `int`, `type_of`, `assert`, `read_file`).

Indirect calls and floating-point values are not lowered yet.
