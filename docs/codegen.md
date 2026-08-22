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
- further arguments on the stack at `[rbp+48]` …
- return value in `rax`
- 32 bytes of home space reserved in every frame
- incoming callee-saved registers are saved in the frame and restored on return

## Register allocation

A linear-scan-style allocator assigns the hottest locals to callee-saved GPRs
(`rbx`, `rsi`, `rdi`, `r13`–`r15`). Remaining locals live in `[rbp]` slots.
Argument/scratch registers stay free for the ABI and instruction selection.

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
| `rt_write_file` | `CreateFileA` + `WriteFile` |
| `rt_trim` / `rt_upper` / `rt_lower` | string helpers |
| `rt_file_exists` / `rt_env` / `rt_cwd` / `rt_remove_file` | OS |

Imports are resolved from `KERNEL32.dll` via a standard PE import table.

## What compiles natively (v0.5 development)

Integers, bools, floats (SSE2), strings, `if`/`while`/`for`/`match`, enums
(as tagged lists), functions (including more than four arguments and indirect
`call r64`), structs, lists, maps, interpolation, `print`, modules, and the
built-in helpers listed above.

Register allocation keeps hot locals in callee-saved registers.

Structured concurrency currently uses a synchronous native fallback: the IR
lowers `spawn f(args)` as the call and `await task` as its underlying value.
This lets typed `conc` programs compile and produce coherent pure results
without claiming that the PE runtime has a scheduler. Scheduling-visible
side-effect order can differ. Interpreter and VM retain real, scope-bound task
handles; a native task runtime is later v0.5 work.
