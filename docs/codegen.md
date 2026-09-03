# Native Multi-Target Backend

Flake's native backend is written entirely in pure Rust. There is no LLVM, Cranelift,
or C transpilation.

```
source → AST → IR → machine code (x86-64 / AArch64) → binary (PE32+ / ELF64)
```

The compiler can target multiple architectures and executable binary formats:
- **x86-64 Windows PE32+** (`x86_64-windows` / `x86_64-windows-pe`)
- **x86-64 Linux ELF64** (`x86_64-linux` / `x86_64-linux-elf`)
- **AArch64 Linux ELF64** (`aarch64-linux` / `aarch64-linux-elf`)

```bash
# Build for host target:
flake build examples/hello.flk -o hello.exe

# Cross-target to Linux ELF:
flake build examples/hello.flk -o hello --target x86_64-linux
flake build examples/hello.flk -o hello_arm64 --target aarch64-linux

# Emit human-readable assembly:
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

### Windows PE Runtime
On Windows, imports are resolved from `KERNEL32.dll` via a standard PE import table (`IAT`).

### Linux ELF Syscall Runtime
On Linux (`x86_64-linux`), the executable contains zero dynamic linker dependencies or libc calls. All OS interactions route through direct Linux syscalls via `flake-codegen/src/runtime_linux.rs`:
- Memory allocation: `sys_mmap` (syscall 9) with `PROT_READ | PROT_WRITE`, `MAP_PRIVATE | MAP_ANONYMOUS`
- File I/O: `sys_open` (2), `sys_read` (0), `sys_write` (1), `sys_close` (3), `sys_unlink` (87)
- Directory reading: `sys_getdents64` (217)
- Process execution & arguments: `sys_fork` (57), `sys_execve` (59), `sys_wait4` (61), `sys_getcwd` (79)
- Termination: `sys_exit` (60)
Generated Linux ELF binaries do not depend on external libraries or interpreters.

### AArch64 Linux ELF (Partial Target)
On AArch64 (`aarch64-linux`), the compiler produces valid 64-bit ELF executables (`EM_AARCH64 = 183`) using pure Rust instruction encoding.
- Arithmetic, register moves, immediate loading, memory load/store, conditional branches, and function calls are fully encoded.
- Advanced systems tooling APIs (`fs`, `process.run`) are implemented as explicit stubs.
- Compilation is verified on all host systems; execution tests run where ARM64 or QEMU runners are present and skip cleanly otherwise.

## What compiles natively in v1.0

Integers, bools, floats (SSE2), strings, `if`/`while`/`for`/`match`, enums
(as tagged lists), functions (including more than four arguments and indirect
`call r64`), structs, lists, maps, interpolation, `print`, modules, constants,
`const fn`, and the built-in helpers listed above.

Result `?` lowers to a tag comparison and an early native return. Scalar
literal patterns lower to ordinary comparisons, without assuming an enum
layout. Native maps select string equality for String keys and direct scalar
equality for Int/Bool keys; construction, duplicate replacement, lookup,
assignment, membership, and display share that representation. Entries remain
in typed-key order through insertion and growth, matching deterministic
interpreter/VM display independent of literal order. Missing-key
lookup is a runtime error, while `contains` remains the non-failing presence
test. Concrete map values retain enough type information for native String,
Int, Bool, and Float display.

Float arithmetic continues to use SSE2, including mixed Int/Float conversion,
remainder, comparisons, and typed `abs`/`min`/`max`. Unary negation flips the
IEEE-754 sign bit, while native `print`, `str`, and interpolation route through
`rt_ftoa` instead of truncating to an integer. Native integer operations report
division-by-zero and overflow failures instead of leaking processor exceptions
or wrapping. Concrete list element types similarly drive deterministic String,
Int, Bool, and Float display.

## Executable production & Binary Writers

Flake includes custom, in-tree binary format writers:
- **PE32+ Writer** (`pe.rs`): Emits Windows 64-bit PE executables, populating headers, section tables, and imports.
- **ELF64 Writer** (`elf.rs`): Emits standalone 64-bit ELF executables for Linux, configuring `PT_LOAD` code and data segments without external linkers.

Output files are written atomically and validated before persisting to disk.

## Task Runtime & Concurrency

Native x86-64 code generation represents `Task[T]` values as heap-allocated
task structures with explicit state tracking (`Pending = 0`, `Joined = 1`, `Running = 2`, `Cancelled = 3`).
The native execution path enforces the strict single-join runtime contract: attempting to
`await` an already-joined task triggers the runtime diagnostic `"task was already awaited"`,
matching interpreter and bytecode VM behavior.

## Assembler Optimizations

The pure-Rust x86-64 assembler applies peephole optimizations:
- Redundant self-moves (`mov reg, reg`) are detected and eliminated.
- Single-assignment variables and constants are folded and propagated at the IR level prior to register allocation.

The [release gate example](../examples/projects/release/main.flk) and [multi-package workspace](../examples/projects/pkg_workspace/app/main.flk) are
end-to-end native showcases for hierarchical modules, public enums, patterns,
Result propagation, maps, and structured tasks:

```bash
flake build examples/projects/release/main.flk -o release.exe
./release.exe
```
