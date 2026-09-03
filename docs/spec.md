# Flake v1.0 Language Specification Index

This document serves as the **normative specification index** for Flake v1.0.0. It defines the architecture of the language specification, references the authoritative document for each subsystem, and specifies stability tiers and compatibility boundaries.

---

## 1. Specification Architecture & Normative Index

The Flake specification is organized into modular documents covering syntax, type/effect semantics, memory safety, execution models, and tooling.

| Subsystem | Normative Specification | Stability Tier | Description |
| :--- | :--- | :--- | :--- |
| **Language Contract** | [docs/stable-subset.md](file:///c:/Users/Izyan/General/.flake/docs/stable-subset.md) | **Tier 1 (Frozen)** | Normative 1.0 language contract and 1.x backwards compatibility guarantee. |
| **Syntax & Grammar** | [docs/grammar.md](file:///c:/Users/Izyan/General/.flake/docs/grammar.md) | **Tier 1 (Frozen)** | Formal EBNF grammar, token definitions, and declaration syntax. |
| **Language Overview** | [docs/tour.md](file:///c:/Users/Izyan/General/.flake/docs/tour.md) | **Tier 1 (Frozen)** | End-to-end language walkthrough: variables, functions, traits, and enums. |
| **Gradual Ownership** | [docs/ownership.md](file:///c:/Users/Izyan/General/.flake/docs/ownership.md) | **Tier 1 (Frozen)** | Linear move tracking, borrow scopes (`&`, `&mut`), and non-escaping references. |
| **Structured Concurrency** | [docs/concurrency.md](file:///c:/Users/Izyan/General/.flake/docs/concurrency.md) | **Tier 1 (Frozen)** | Scope-bound `Task[T]`, nurseries, `spawn`/`await`, and sendability invariants. |
| **CTFE Lite** | [docs/ctfe.md](file:///c:/Users/Izyan/General/.flake/docs/ctfe.md) | **Tier 1 (Frozen)** | Pure `const fn`, `const` items, safety fuel, and recursion limits. |
| **Standard Library** | [docs/stdlib.md](file:///c:/Users/Izyan/General/.flake/docs/stdlib.md) | **Tier 1 (Frozen)** | Normative API reference for core modules (`fs`, `path`, `process`, `list`, etc.). |
| **Packages & Locking** | [docs/packages.md](file:///c:/Users/Izyan/General/.flake/docs/packages.md) | **Tier 1 (Frozen)** | `flake.toml` manifests, dependency resolution, and deterministic `flake.lock`. |
| **Code Generation** | [docs/codegen.md](file:///c:/Users/Izyan/General/.flake/docs/codegen.md) | **Tier 1 / Tier 2** | Machine code emission, PE32+ and Linux ELF syscall runtimes, target matrix. |
| **Bootstrap Loop** | [docs/bootstrap.md](file:///c:/Users/Izyan/General/.flake/docs/bootstrap.md) | **Tier 1 (Frozen)** | Self-check verification, deterministic rebuild, and `flake bootstrap` CLI. |
| **Quality Gate** | [docs/testing.md](file:///c:/Users/Izyan/General/.flake/docs/testing.md) | **Tier 1 (Frozen)** | Release test checklist and backend consistency requirements. |

---

## 2. Stability Tiers

1. **Tier 1 (Frozen & Stable)**:
   - Full backward compatibility guaranteed across all 1.x releases.
   - Code conforming to Tier 1 that compiles with `flake check` in 1.0.0 will continue to compile and preserve identical runtime semantics in 1.1.0, 1.2.0, etc.
   - Includes all items in [docs/stable-subset.md](file:///c:/Users/Izyan/General/.flake/docs/stable-subset.md), Windows PE32+ native execution, and Linux ELF64 native execution.

2. **Tier 2 (Partial / Evolutionary)**:
   - Validated for cross-compilation and executable format generation, but runtime or systems integration is partial.
   - Includes **`aarch64-linux`**: 64-bit ELF emission and core instruction encodings exist; systems APIs are explicit stubs. Full native systems runtime on AArch64 is scheduled for future 1.x point releases.

---

## 3. Explicit Non-Goals & Boundaries

The following capabilities are deliberately excluded from Flake v1.0 and will not be introduced in 1.x:

- **Macro Systems**: No AST transformation macros, token tree quotation, or procedural macros.
- **Compile-Time I/O**: CTFE is strictly pure and cannot access disks, networks, or processes.
- **Host Compiler Replacement**: The pure Rust compiler pipeline remains the authoritative compiler of record.
- **Flake-Written Codegen**: The machine code emitter remains in Rust.
- **Alternative Runtimes**: No C transpilation, LLVM, or Cranelift dependencies.
