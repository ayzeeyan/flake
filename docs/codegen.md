# Native x86-64 backend

Flake's native backend is written entirely in-tree. There is no LLVM, Cranelift,
or C transpilation.

```
source → AST → IR → x86-64 machine code → PE32+ executable
```

On Windows the compiler emits a real `.exe`. Assembly listings are explicit,
optional diagnostic artifacts. `flake run --native` compiles to a temporary
executable, runs it, and removes it on both success and failure.

```bash
flake build examples/hello.flk -o hello.exe
flake build examples/hello.flk -o hello.exe --emit-asm
flake run --native examples/hello.flk
```

`flake build` always type-checks first. By default it creates only `hello.exe`;
`--emit-asm` additionally creates `hello.s`.

## Calling convention

Generated functions use the **Windows x64 ABI**:

- integer/pointer arguments in `rcx`, `rdx`, `r8`, `r9`
- further arguments on the stack at `[rbp+48]` …
- return value in `rax`
- 32 bytes of home space reserved in every frame
- incoming callee-saved registers are saved in the frame and restored on return

## Register allocation

The allocator computes backwards liveness over the IR control-flow graph,
builds an interference graph, and greedily colors the hottest locals onto
callee-saved GPRs (`rbx`, `rsi`, `rdi`, `r13`–`r15`). Non-overlapping locals
can reuse a register; remaining locals live in `[rbp]` slots. Concurrent
instruction operands are kept distinct where lowering needs them at the same
time. Argument/scratch registers stay free for the ABI and instruction
selection.

Function values lower to typed `fnaddr` locals. Direct and indirect calls use
the same argument staging, home-space, stack-argument, and return-value rules,
so a call through a local function value supports the complete current Flake
calling convention rather than only the machine-code `call r64` encoding.
User and imported functions can be materialized this way. Builtins whose
lowering depends on argument types should be placed behind a user-function
wrapper; attempting to take their native address reports that guidance.

## Runtime

A small hand-written runtime is linked into every image:

| Symbol | Role |
| --- | --- |
| `rt_print_cstr` | `WriteFile` a C string |
| `rt_print_i64` | decimal print |
| `rt_concat2` | heap-concatenate two C strings |
| `rt_itoa` | integer → heap C string |
| `rt_ftoa` | Float → deterministic decimal C string |
| `rt_alloc` | `HeapAlloc` |
| `rt_join` | join a list of strings |
| `rt_streq` / `rt_split` | string equality and split |
| `rt_list_*` / `rt_map_*` | growable lists and typed-key linear maps |
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

Result `?` lowers to a tag comparison and an early native return. Scalar
literal patterns lower to ordinary comparisons, without assuming an enum
layout. Native maps select string equality for String keys and direct scalar
equality for Int/Bool keys; construction, duplicate replacement, lookup,
assignment, membership, and display share that representation. Missing-key
lookup is a runtime error, while `contains` remains the non-failing presence
test. Concrete map values retain enough type information for native String,
Int, Bool, and Float display.

Float arithmetic continues to use SSE2. Unary negation flips the IEEE-754 sign
bit, while native `print`, `str`, and interpolation route through `rt_ftoa`
instead of truncating to an integer.

## Executable production

The PE writer records actual virtual section sizes plus aligned raw sizes and
fills `SizeOfCode`, `SizeOfInitializedData`, and `BaseOfCode`. Output is first
written and flushed to a unique sibling file, then installed at the requested
path; an existing output is backed up and restored if replacement fails. The
default build no longer leaves an unsolicited assembly sidecar.

Structured concurrency currently uses a synchronous native fallback: the IR
lowers `spawn f(args)` as the call and `await task` as its underlying value.
This lets typed `conc` programs compile and produce coherent pure results
without claiming that the PE runtime has a scheduler. Scheduling-visible
side-effect order can differ. Interpreter and VM retain real, scope-bound task
handles; a native task runtime is later v0.5 work.
