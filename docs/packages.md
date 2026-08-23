# Flake Packages and Manifests

Flake v0.5.6 introduces first-class package management foundations, allowing projects to be structured as local packages with clear manifest declarations and dependency graphs.

## The Package Manifest (`flake.toml`)

Every Flake package is configured with a `flake.toml` file at its root directory:

```toml
[package]
name = "my_service"
version = "0.1.0"
entry = "main.flk" # optional, defaults to main.flk
description = "My high-performance service"
authors = ["Flake Developer <dev@flake.lang>"]

[dependencies]
core_utils = { path = "../core_utils" }
telemetry = { path = "../../common/telemetry" }
```

### Manifest Fields

- `package.name`: The package identifier (used for import resolution and diagnostics).
- `package.version`: Semantic version string.
- `package.entry`: The entrypoint `.flk` source file (defaults to `main.flk`).
- `package.description`: Optional human-readable description.
- `package.authors`: Optional list of author strings.
- `dependencies`: Map of dependency names to dependency specifications (`{ path = "..." }`).

## Creating Packages

Flake CLI provides built-in commands to create and initialize packages:

### Initialize in current directory
```bash
flake init
# Or specify a custom package name:
flake init --name my_app
```
Creates `flake.toml` and `main.flk` in the current working directory.

### Create a new package directory
```bash
flake new path/to/my_package
```
Creates the target directory containing `flake.toml` and a starter `main.flk`.

## Running and Building Packages

You can invoke Flake commands directly on package directories or within a package root:

```bash
# In the directory containing flake.toml:
flake run
flake run --vm
flake run --native
flake check
flake build -o service.exe

# Target a package directory explicitly:
flake run path/to/my_package
flake check path/to/my_package
flake build path/to/my_package -o dist/app.exe
```

## Multi-Package Workspaces

Packages can depend on each other via relative file paths:

```
workspace/
├── core_lib/
│   ├── flake.toml
│   ├── main.flk
│   └── service.flk
└── app/
    ├── flake.toml
    └── main.flk
```

In `app/flake.toml`:
```toml
[package]
name = "app"
version = "0.1.0"
entry = "main.flk"

[dependencies]
core_lib = { path = "../core_lib" }
```

In `app/main.flk`:
```flake
import core_lib
import core_lib.service

fn main() / io {
    print(core_lib.format_greeting("Flake"))
    print(service.compute_metrics(10, 5))
}
```

## Dependency Resolution Rules

1. When importing a symbol (`import foo`), the compiler first searches ancestor directories for a `flake.toml`.
2. If `foo` is defined in `[dependencies]`, the path is resolved relative to the package directory.
3. If importing a submodule of a package (`import foo.bar`), the compiler locates `bar.flk` within the dependency package folder.
4. Circular dependencies across packages or modules are detected and reported with actionable diagnostics.
