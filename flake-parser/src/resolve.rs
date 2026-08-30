//! Load a Flake program and the `.flk` files it `import`s.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use flake_ast::{ImportDecl, Item, Program, Source, Span};

use crate::error::ParseError;
use crate::parser::parse;

/// One parsed `.flk` file in a module graph.
#[derive(Debug, Clone)]
pub struct LoadedModule {
    pub name: String,
    pub source: Source,
    pub program: Program,
}

/// Entry module plus every imported module.
#[derive(Debug, Clone)]
pub struct ModuleGraph {
    /// Index 0 is always the entry file.
    pub modules: Vec<LoadedModule>,
    index: HashMap<String, usize>,
    imports: HashMap<(String, String), String>,
}

impl ModuleGraph {
    #[must_use]
    pub fn entry(&self) -> &LoadedModule {
        &self.modules[0]
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&LoadedModule> {
        self.index
            .get(name)
            .and_then(|index| self.modules.get(*index))
    }

    /// Resolve one parsed import from its containing module.
    #[must_use]
    pub fn imported(&self, importer: &LoadedModule, import: &ImportDecl) -> Option<&LoadedModule> {
        let key = (importer.name.clone(), import.path.name.clone());
        self.imports.get(&key).and_then(|name| self.get(name))
    }

    /// Collect all exported items of a module, including transitive re-exports through `pub import`.
    #[must_use]
    pub fn exported_items<'a>(
        &'a self,
        module: &'a LoadedModule,
    ) -> Vec<(&'a Item, &'a LoadedModule)> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        self.collect_exported_items(module, &mut visited, &mut result);
        result
    }

    fn collect_exported_items<'a>(
        &'a self,
        module: &'a LoadedModule,
        visited: &mut HashSet<String>,
        result: &mut Vec<(&'a Item, &'a LoadedModule)>,
    ) {
        if !visited.insert(module.name.clone()) {
            return;
        }
        for item in &module.program.items {
            if is_exported(item, &module.program) {
                result.push((item, module));
                if let Item::Import(pub_import) = item {
                    if let Some(sub) = self.imported(module, pub_import) {
                        self.collect_exported_items(sub, visited, result);
                    }
                }
            }
        }
    }

    /// Whether an exported declaration can also be used as an unqualified
    /// name in `module`. Qualified `alias.name` access is always available.
    #[must_use]
    pub fn unqualified_import_is_unambiguous(&self, module: &LoadedModule, export: &str) -> bool {
        self.import_aliases_for_export(module, export).len() == 1
    }

    /// Import aliases that export `name` into `module`.
    #[must_use]
    pub fn import_aliases_for_export(&self, module: &LoadedModule, export: &str) -> Vec<String> {
        module
            .program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Import(import) => Some(import),
                _ => None,
            })
            .filter_map(|import| {
                let imported = self.imported(module, import)?;
                let exports = self.exported_items(imported);
                exports
                    .iter()
                    .any(|(item, _origin)| item_name(item) == Some(export))
                    .then(|| import_alias(import).to_string())
            })
            .collect()
    }

    /// Every imported name that requires qualified access because multiple
    /// module aliases export it.
    #[must_use]
    pub fn ambiguous_imports(&self, module: &LoadedModule) -> HashMap<String, Vec<String>> {
        let mut owners: HashMap<String, Vec<String>> = HashMap::new();
        for item in &module.program.items {
            let Item::Import(import) = item else {
                continue;
            };
            let Some(imported) = self.imported(module, import) else {
                continue;
            };
            let alias = import_alias(import).to_string();
            let exports = self.exported_items(imported);
            for (item, _origin) in exports {
                if let Some(name) = item_name(item) {
                    let aliases = owners.entry(name.to_string()).or_default();
                    if !aliases.contains(&alias) {
                        aliases.push(alias.clone());
                    }
                }
            }
        }
        owners.retain(|_, aliases| aliases.len() > 1);
        owners
    }
}

/// Failure to read, parse, or resolve `import`s.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct ResolveError {
    pub span: Span,
    pub message: String,
    /// Set when the error belongs to a file other than the original entry.
    pub origin: Option<Source>,
}

impl ResolveError {
    fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
            origin: None,
        }
    }

    fn with_source(mut self, source: Source) -> Self {
        self.origin = Some(source);
        self
    }
}

impl From<ParseError> for ResolveError {
    fn from(err: ParseError) -> Self {
        Self::new(err.span, err.message)
    }
}

/// Parse `source` and recursively load sibling or dotted project modules.
pub fn load_graph(source: &Source) -> Result<ModuleGraph, ResolveError> {
    let mut modules = Vec::new();
    let mut imports = HashMap::new();
    let mut stack = Vec::new();
    let name = module_stem(source.name());
    let root = source_dir(source.name());
    load_one(
        source.clone(),
        name,
        &root,
        &mut modules,
        &mut imports,
        &mut stack,
    )?;
    if let Some(entry) = modules.pop() {
        modules.insert(0, entry);
    }
    let index = modules
        .iter()
        .enumerate()
        .map(|(index, module)| (module.name.clone(), index))
        .collect();
    Ok(ModuleGraph {
        modules,
        index,
        imports,
    })
}

/// Bind name used in the importing file (`as` alias, or the module ident).
#[must_use]
pub fn import_alias(import: &ImportDecl) -> &str {
    import
        .alias
        .as_ref()
        .map(|a| a.name.as_str())
        .unwrap_or_else(|| {
            import
                .path
                .name
                .rsplit('.')
                .next()
                .unwrap_or(import.path.name.as_str())
        })
}

/// Qualified function/struct name: `math.add`.
#[must_use]
pub fn qualify(module: &str, name: &str) -> String {
    format!("{module}.{name}")
}

/// Imported modules expose only declarations explicitly marked `pub`.
#[must_use]
pub fn is_exported(item: &Item, _program: &Program) -> bool {
    item.is_pub()
}

fn item_name(item: &Item) -> Option<&str> {
    match item {
        Item::Fn(item) => Some(&item.name.name),
        Item::Struct(item) => Some(&item.name.name),
        Item::Enum(item) => Some(&item.name.name),
        Item::Type(item) => Some(&item.name.name),
        Item::Trait(item) => Some(&item.name.name),
        Item::Import(item) => item.is_pub.then(|| import_alias(item)),
        Item::Impl(_) => None,
    }
}

fn load_one(
    source: Source,
    name: String,
    project_root: &Path,
    modules: &mut Vec<LoadedModule>,
    imports: &mut HashMap<(String, String), String>,
    stack: &mut Vec<String>,
) -> Result<(), ResolveError> {
    if modules.iter().any(|m| m.name == name) {
        return Ok(());
    }
    stack.push(name.clone());
    let program = parse(&source).map_err(|err| {
        let src = source.clone();
        ResolveError::from(err).with_source(src)
    })?;
    validate_import_bindings(&source, &program)?;
    for item in &program.items {
        if let Item::Import(import) = item {
            let resolved = find_module(
                source.name(),
                &import.path.name,
                project_root,
            )
            .ok_or_else(|| {
                ResolveError::new(
                    import.span,
                    format!(
                        "cannot find module `{}`\nhelp: expected `{}` under project root `{}` or in a `std/` directory",
                        import.path.name,
                        module_relative_path(&import.path.name).display(),
                        project_root.display()
                    ),
                )
                .with_source(source.clone())
            })?;
            imports.insert(
                (name.clone(), import.path.name.clone()),
                resolved.name.clone(),
            );
            if let Some(start) = stack.iter().position(|module| module == &resolved.name) {
                let mut cycle = stack[start..].to_vec();
                cycle.push(resolved.name.clone());
                return Err(ResolveError::new(
                    import.span,
                    format!("cyclic import: {}", cycle.join(" -> ")),
                )
                .with_source(source.clone()));
            }
            let text = fs::read_to_string(&resolved.path).map_err(|e| {
                ResolveError::new(
                    import.span,
                    format!("cannot read {}: {e}", resolved.path.display()),
                )
                .with_source(source.clone())
            })?;
            let child = Source::new(resolved.path.display().to_string(), text);
            let next_root = if resolved.path.starts_with(project_root) {
                project_root
            } else {
                resolved.path.parent().unwrap_or(project_root)
            };
            load_one(child, resolved.name, next_root, modules, imports, stack)?;
        }
    }
    stack.pop();
    modules.push(LoadedModule {
        name,
        source,
        program,
    });
    Ok(())
}

fn decl_name(item: &Item) -> Option<&str> {
    match item {
        Item::Fn(item) => Some(&item.name.name),
        Item::Struct(item) => Some(&item.name.name),
        Item::Enum(item) => Some(&item.name.name),
        Item::Type(item) => Some(&item.name.name),
        Item::Trait(item) => Some(&item.name.name),
        Item::Import(_) | Item::Impl(_) => None,
    }
}

fn validate_import_bindings(source: &Source, program: &Program) -> Result<(), ResolveError> {
    let declarations: HashSet<_> = program.items.iter().filter_map(decl_name).collect();
    let mut aliases = HashSet::new();
    let mut paths = HashSet::new();
    for item in &program.items {
        let Item::Import(import) = item else {
            continue;
        };
        let alias = import_alias(import);
        if !aliases.insert(alias) {
            return Err(ResolveError::new(
                import.span,
                format!(
                    "duplicate import alias `{alias}`\nhelp: use `as` to give each module a distinct name"
                ),
            )
            .with_source(source.clone()));
        }
        if !paths.insert(import.path.name.as_str()) {
            return Err(ResolveError::new(
                import.span,
                format!("module `{}` is imported more than once", import.path.name),
            )
            .with_source(source.clone()));
        }
        if declarations.contains(alias) {
            return Err(ResolveError::new(
                import.span,
                format!(
                    "import alias `{alias}` conflicts with a top-level declaration\nhelp: choose a distinct alias with `as`"
                ),
            )
            .with_source(source.clone()));
        }
    }
    Ok(())
}

struct ResolvedModule {
    path: PathBuf,
    name: String,
}

fn find_module(importer: &str, module: &str, project_root: &Path) -> Option<ResolvedModule> {
    let relative = module_relative_path(module);
    let importer_dir = source_dir(importer);

    // 0. Package Manifest Dependency Lookup (flake.toml)
    if let Some((manifest_dir, manifest)) =
        crate::manifest::Manifest::find_in_ancestors(&importer_dir)
    {
        let (root_pkg, rest) = match module.split_once('.') {
            Some((first, rest)) => (first, Some(rest)),
            None => (module, None),
        };

        if let Some(dep_path) = manifest.resolve_dependency_path(&manifest_dir, root_pkg) {
            if let Some((dep_manifest_dir, dep_manifest)) =
                crate::manifest::Manifest::find_in_ancestors(&dep_path)
            {
                if let Some(sub) = rest {
                    let sub_rel = module_relative_path(sub);
                    let candidate = dep_manifest_dir.join(&sub_rel);
                    if candidate.is_file() {
                        return Some(ResolvedModule {
                            name: format!("{root_pkg}.{sub}"),
                            path: candidate,
                        });
                    }
                    let candidate_src = dep_manifest_dir.join("src").join(&sub_rel);
                    if candidate_src.is_file() {
                        return Some(ResolvedModule {
                            name: format!("{root_pkg}.{sub}"),
                            path: candidate_src,
                        });
                    }
                } else {
                    let entry = dep_manifest_dir.join(&dep_manifest.package.entry);
                    if entry.is_file() {
                        return Some(ResolvedModule {
                            name: root_pkg.to_string(),
                            path: entry,
                        });
                    }
                }
            } else if let Some(sub) = rest {
                let sub_rel = module_relative_path(sub);
                let candidate = dep_path.join(&sub_rel);
                if candidate.is_file() {
                    return Some(ResolvedModule {
                        name: format!("{root_pkg}.{sub}"),
                        path: candidate,
                    });
                }
            } else {
                if dep_path.is_file() {
                    return Some(ResolvedModule {
                        name: root_pkg.to_string(),
                        path: dep_path,
                    });
                }
                for name in ["main.flk", "lib.flk", "index.flk"] {
                    let candidate = dep_path.join(name);
                    if candidate.is_file() {
                        return Some(ResolvedModule {
                            name: root_pkg.to_string(),
                            path: candidate,
                        });
                    }
                }
                let flk_candidate = dep_path.with_extension("flk");
                if flk_candidate.is_file() {
                    return Some(ResolvedModule {
                        name: root_pkg.to_string(),
                        path: flk_candidate,
                    });
                }
            }
        }
    }

    // 1. Sibling or importer-relative lookup (e.g. `import math` or `import sub.module`)
    let importer_relative = importer_dir.join(&relative);
    if importer_relative.is_file() {
        return Some(ResolvedModule {
            name: logical_module_name(&importer_relative, project_root, module),
            path: importer_relative,
        });
    }

    // 2. Project root lookup (e.g. `import domain.pricing` from deep subdirectories)
    let project = project_root.join(&relative);
    if project.is_file() {
        return Some(ResolvedModule {
            name: logical_module_name(&project, project_root, module),
            path: project,
        });
    }

    // 3. Walk up searching for `std/` directory
    let mut dir = importer_dir;
    loop {
        let candidate = dir.join("std").join(&relative);
        if candidate.is_file() {
            return Some(ResolvedModule {
                name: logical_module_name(&candidate, project_root, &std_module_name(module)),
                path: candidate,
            });
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => break,
        }
    }
    if let Ok(root) = std::env::var("FLAKE_STD") {
        let candidate = PathBuf::from(root).join(&relative);
        if candidate.is_file() {
            return Some(ResolvedModule {
                name: std_module_name(module),
                path: candidate,
            });
        }
    }
    None
}

fn module_relative_path(module: &str) -> PathBuf {
    let mut path = PathBuf::new();
    for segment in module.split('.') {
        path.push(segment);
    }
    path.set_extension("flk");
    path
}

fn source_dir(source_name: &str) -> PathBuf {
    Path::new(source_name)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn logical_module_name(path: &Path, project_root: &Path, fallback: &str) -> String {
    if project_root.as_os_str().is_empty() || project_root == Path::new(".") {
        if let Some(name) = path_to_module_name(path) {
            return name;
        }
    }
    path.strip_prefix(project_root)
        .ok()
        .and_then(path_to_module_name)
        .unwrap_or_else(|| fallback.to_string())
}

fn path_to_module_name(path: &Path) -> Option<String> {
    let mut path = path.to_path_buf();
    path.set_extension("");
    let segments: Vec<_> = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(segment) => segment.to_str(),
            _ => None,
        })
        .collect();
    (!segments.is_empty()).then(|| segments.join("."))
}

fn std_module_name(module: &str) -> String {
    if module.starts_with("std.") {
        module.to_string()
    } else {
        format!("std.{module}")
    }
}

fn module_stem(source_name: &str) -> String {
    Path::new(source_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("main")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempProject(PathBuf);

    impl TempProject {
        fn new(label: &str) -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "flake-resolve-{label}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create test project");
            Self(path)
        }

        fn write(&self, relative: &str, source: &str) -> PathBuf {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create module directory");
            }
            fs::write(&path, source).expect("write module");
            path
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn single_file_has_one_module() {
        let source = Source::new("t.flk", "fn main() { print(1) }");
        let graph = load_graph(&source).expect("load");
        assert_eq!(graph.modules.len(), 1);
        assert_eq!(graph.entry().name, "t");
    }

    #[test]
    fn missing_import_is_an_error() {
        let source = Source::new("t.flk", "import nope\nfn main() {}");
        let err = load_graph(&source).expect_err("missing module");
        assert!(err.message.contains("nope"), "{}", err.message);
    }

    #[test]
    fn dotted_imports_are_project_relative_and_canonical() {
        let project = TempProject::new("dotted");
        project.write(
            "services/checkout.flk",
            "import domain.pricing\npub fn total() -> Int { pricing.price() }",
        );
        project.write("domain/pricing.flk", "pub fn price() -> Int { 42 }");
        let main_path = project.write(
            "main.flk",
            "import services.checkout\nfn main() { checkout.total() }",
        );
        let text = fs::read_to_string(&main_path).expect("read main");
        let graph = load_graph(&Source::new(main_path.display().to_string(), text)).expect("load");

        assert!(graph.get("services.checkout").is_some());
        assert!(graph.get("domain.pricing").is_some());
        let Item::Import(import) = &graph.entry().program.items[0] else {
            panic!("expected import");
        };
        assert_eq!(
            graph
                .imported(graph.entry(), import)
                .map(|module| module.name.as_str()),
            Some("services.checkout")
        );
        assert_eq!(import_alias(import), "checkout");
    }

    #[test]
    fn repeated_file_stems_keep_distinct_module_identities() {
        let project = TempProject::new("identities");
        project.write("alpha/util.flk", "pub fn value() -> Int { 1 }");
        project.write("beta/util.flk", "pub fn value() -> Int { 2 }");
        let main_path = project.write(
            "main.flk",
            "import alpha.util as alpha\nimport beta.util as beta\nfn main() {}",
        );
        let text = fs::read_to_string(&main_path).expect("read main");
        let graph = load_graph(&Source::new(main_path.display().to_string(), text)).expect("load");

        assert!(graph.get("alpha.util").is_some());
        assert!(graph.get("beta.util").is_some());
        assert_eq!(
            graph.ambiguous_imports(graph.entry()).get("value"),
            Some(&vec!["alpha".to_string(), "beta".to_string()])
        );
    }

    #[test]
    fn duplicate_import_aliases_are_rejected() {
        let source = Source::new(
            "main.flk",
            "import alpha.util\nimport beta.util\nfn main() {}",
        );
        let error = load_graph(&source).expect_err("duplicate alias");
        assert!(error.message.contains("duplicate import alias `util`"));
        assert!(error.message.contains("use `as`"));
    }

    #[test]
    fn duplicate_paths_and_declaration_conflicts_are_rejected() {
        let duplicate = Source::new(
            "main.flk",
            "import alpha as first\nimport alpha as second\nfn main() {}",
        );
        let error = load_graph(&duplicate).expect_err("duplicate path");
        assert!(error.message.contains("imported more than once"));

        let conflict = Source::new(
            "main.flk",
            "import services.checkout as run\nfn run() {}\nfn main() {}",
        );
        let error = load_graph(&conflict).expect_err("declaration conflict");
        assert!(
            error
                .message
                .contains("conflicts with a top-level declaration")
        );
    }

    #[test]
    fn cycles_report_the_full_module_chain_and_origin() {
        let project = TempProject::new("cycle");
        project.write("a.flk", "import b\npub fn a() { b.b() }");
        project.write("b.flk", "import a\npub fn b() { a.a() }");
        let main_path = project.write("main.flk", "import a\nfn main() { a.a() }");
        let text = fs::read_to_string(&main_path).expect("read main");
        let error = load_graph(&Source::new(main_path.display().to_string(), text))
            .expect_err("cycle should fail");

        assert!(error.message.contains("a -> b -> a"), "{}", error.message);
        assert!(
            error
                .origin
                .as_ref()
                .is_some_and(|source| source.name().ends_with("b.flk"))
        );
    }

    #[test]
    fn package_manifest_resolves_local_dependencies() {
        let root = TempProject::new("pkg-deps");
        // Create math library package
        root.write(
            "libs/math_pkg/flake.toml",
            r#"
[package]
name = "math_pkg"
version = "0.1.0"
entry = "src/lib.flk"
"#,
        );
        root.write(
            "libs/math_pkg/src/lib.flk",
            "pub fn add(a: Int, b: Int) -> Int { a + b }\npub fn mul(a: Int, b: Int) -> Int { a * b }",
        );

        // Create main app package
        root.write(
            "app/flake.toml",
            r#"
[package]
name = "app"
version = "0.1.0"
entry = "main.flk"

[dependencies]
math = { path = "../libs/math_pkg" }
"#,
        );
        let main_path = root.write(
            "app/main.flk",
            r#"
import math

fn main() {
    let sum = math.add(10, 20)
    let prod = math.mul(3, 4)
    print(sum)
    print(prod)
}
"#,
        );

        let text = fs::read_to_string(&main_path).expect("read main");
        let graph = load_graph(&Source::new(main_path.display().to_string(), text)).expect("load");
        assert!(
            graph.get("math").is_some(),
            "expected module `math` in graph"
        );
        assert_eq!(graph.modules.len(), 2);
    }
}
