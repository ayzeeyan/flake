# Modules and multi-file projects

Flake treats every `.flk` file as one module. v0.5 milestone 4 adds
project-rooted dotted imports, canonical module identities, strict visibility,
and deterministic name resolution across the interpreter, VM, and native
backend.

## Project layout

The directory containing the entry file is the project root. A dotted import
maps each name segment to a directory and adds the `.flk` extension:

```text
shop/
├── main.flk
├── domain/
│   └── pricing.flk
└── services/
    └── checkout.flk
```

```flake
// main.flk
import domain.pricing as pricing
import services.checkout as checkout

fn main() {
    let tier = pricing.Tier.Premium
    print(checkout.order_total(tier, 4))
}
```

`domain.pricing` resolves to `domain/pricing.flk` beneath the project root.
The canonical module identity is also `domain.pricing`; two files such as
`left/util.flk` and `right/util.flk` therefore remain distinct.

For a one-segment import such as `import helper`, Flake first checks next to
the importing file. This lets a nested module keep a private group of sibling
files. It then checks the project root. Standard-library lookup walks ancestor
`std/` directories and finally checks `FLAKE_STD` when that variable is set.

## Aliases and namespaces

Without `as`, the last path segment becomes the namespace:

```flake
import services.checkout        // namespace: checkout
import services.checkout as pay // namespace: pay
```

Public values are always available through the namespace. Public types and
enum patterns can use the namespace too:

```flake
fn fee(tier: pricing.Tier) -> Int {
    match tier {
        pricing.Tier.Standard => 2
        pricing.Tier.Premium => 6
    }
}
```

Qualified struct construction follows the same rule:

```flake
let samples: stats.SampleSet = stats.SampleSet { total: 100, count: 4 }
```

An exported name is also available bare when exactly one imported module
exports it. If more than one import exports `value`, `value()` is rejected as
ambiguous and the diagnostic lists choices such as `left.value()` and
`right.value()`. Qualified access remains deterministic. A declaration in the
current file takes precedence over an imported bare name.

Import aliases must be unique within a file. Importing the same path twice or
choosing an alias that conflicts with a top-level declaration is an error.

## Visibility

Imported declarations are private by default. Mark every API item explicitly:

```flake
fn calculate_tax(subtotal: Int) -> Int { // module-private helper
    subtotal / 10
}

pub fn total(subtotal: Int) -> Int {      // visible to importers
    subtotal + calculate_tax(subtotal)
}
```

`pub` is supported on functions, structs, enums, and type aliases. There is no
legacy “export everything when no `pub` appears” fallback in v0.5. Private
helpers remain visible throughout their own module and cannot leak through a
namespace or bare import.

A public signature cannot expose a private local type. The checker points at
the leaked type and asks you either to mark that type `pub` or keep the API
item private.

The interpreter gives each module its own lexical environment. The VM and
native paths qualify module functions with their canonical identities. Thus,
private helpers with the same spelling in separate files cannot overwrite or
capture one another.

## Cycles and diagnostics

The module graph must be acyclic. Cycle errors show the complete relevant
chain, for example:

```text
cyclic import: domain.a -> services.b -> domain.a
```

Missing-module errors point at the importing file and show the project-relative
path Flake expected. Visibility errors suggest adding `pub`; ambiguous bare
names suggest explicit qualified alternatives.

## Packages and Local Dependencies

In Flake v0.5.6, projects can define package manifests with `flake.toml` and depend on other local packages:

```toml
[package]
name = "my_app"
version = "0.1.0"
entry = "main.flk"

[dependencies]
core_lib = { path = "../core_lib" }
```

When importing `import core_lib` or `import core_lib.service`, the compiler locates the dependency directory and resolves the library root or submodule accordingly. See [packages.md](packages.md) for complete details.

## Runnable projects

- [Inventory](../examples/projects/inventory/main.flk) demonstrates a public
  domain enum, qualified types, and a service layered over that domain.
- [Telemetry](../examples/projects/telemetry/main.flk) demonstrates transitive
  imports and same-named private helpers isolated in separate modules.
- [Release gate](../examples/projects/release/main.flk) combines public enums,
  a Result-like service API, maps, and structured tasks in a native-ready
  application.
- [Multi-package Workspace](../examples/projects/pkg_workspace/app/main.flk) demonstrates
  `flake.toml` package dependency declaration and consumption.

Run a project by passing its entry file or its directory:

```bash
flake run examples/projects/release/main.flk
flake run --vm examples/projects/release/main.flk
flake run --native examples/projects/release/main.flk
flake run examples/projects/pkg_workspace/app
```
