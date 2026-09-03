# Compile-Time Function Evaluation (CTFE Lite)

Flake v0.13 introduces **CTFE lite**: compile-time evaluation of constant declarations and pure constant functions.

CTFE lite provides deterministic compile-time evaluation without introducing macro systems, procedural expansion, or arbitrary compile-time execution.

---

## 1. Syntax

### Constant Declarations
Constants are declared at the top-level using the `const` keyword:

```flake
const BUFFER_SIZE: Int = 1024 * 4
const GREETING: String = "Hello" + ", world!"
const IS_DEBUG: Bool = false
```

### Constant Functions (`const fn`)
Functions marked with `const` can be evaluated at compile time when called from constant expressions:

```flake
const fn double(x: Int) -> Int {
    x * 2
}

const fn max_val(a: Int, b: Int) -> Int {
    if a >= b { a } else { b }
}

const LIMIT: Int = double(max_val(10, 20)) + 1 // evaluates to 41
```

`const fn` functions can also be called as regular functions at runtime.

---

## 2. Supported Compile-Time Operations

The CTFE evaluator supports:

- **Literals**: Int, Float, Bool, String, and Nil.
- **Arithmetic**: `+`, `-`, `*`, `/`, `%`, and unary `-`.
- **Logical Operations**: `&&`, `||`, and unary `!`. Short-circuit evaluation applies.
- **Comparisons**: `==`, `!=`, `<`, `>`, `<=`, `>=`.
- **Conditionals**: `if cond { expr } else { expr }` where `cond` is a constant Bool expression. An `else` branch is mandatory in const expressions.
- **String Operations**: String concatenation (`+`) and string interpolation (`"{VAL}"`).
- **Function Calls**: Calls to declared `const fn` functions whose arguments evaluate to constants. Const functions may call other `const fn`s.
- **Block Expressions**: Blocks containing a tail expression (`{ expr }`).

---

## 3. Strict Purity and Safety Guarantees

CTFE lite is strictly sandboxed and side-effect free:

| Category | Status | Details |
| :--- | :--- | :--- |
| **I/O Operations** | **Forbidden** | Calling `read_file`, `write_file`, `print`, etc. is rejected at compile time. |
| **Process Execution** | **Forbidden** | `run_cmd`, `args`, `env`, etc. are rejected. |
| **Concurrency** | **Forbidden** | `spawn`, `await`, nurseries, and channels are rejected. |
| **Non-Const Calls** | **Forbidden** | Calling any non-`const` function from a const expression results in a compiler error. |
| **Statements in Const**| **Forbidden** | Const blocks cannot contain variable declarations, loops, or statements. |
| **Effects** | **Forbidden** | `const fn` cannot declare impure effects (`/ io`, `/ conc`, etc.). Only pure functions are permitted. |

---

## 4. Termination and Limits

To prevent infinite loops or deep recursion from hanging the compiler:

1. **CTFE Fuel (`10,000` steps)**:
   Every function call and evaluation step consumes fuel from a bounded quota. If fuel is exhausted, compilation fails with:
   ```
   error: const evaluation exceeded recursion limit
   ```

2. **Call Depth Limit (`256` frames)**:
   Maximum call recursion depth is capped at 256 stack frames, preventing stack overflow on any host operating system.

---

## 5. Backend Parity

Const items are folded at type-checking and IR lowering time:

- **Interpreter**: Reads pre-folded constant values directly.
- **Bytecode VM**: Emits literal bytecode instructions for the folded constants.
- **Native Backend**: Inlines folded constant values into generated machine code.

All backends yield identical values for all constant expressions.
