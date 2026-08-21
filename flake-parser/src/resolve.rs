//! Load a Flake program and the `.flk` files it `import`s.

use std::collections::HashSet;
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
}

impl ModuleGraph {
    #[must_use]
    pub fn entry(&self) -> &LoadedModule {
        &self.modules[0]
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&LoadedModule> {
        self.modules.iter().find(|m| m.name == name)
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

/// Parse `source` and recursively load `import ident` as sibling `.flk` files.
pub fn load_graph(source: &Source) -> Result<ModuleGraph, ResolveError> {
    let mut modules = Vec::new();
    let mut loading = HashSet::new();
    let name = module_stem(source.name());
    load_one(source.clone(), name, None, &mut modules, &mut loading)?;
    if let Some(entry) = modules.pop() {
        modules.insert(0, entry);
    }
    Ok(ModuleGraph { modules })
}

/// Bind name used in the importing file (`as` alias, or the module ident).
#[must_use]
pub fn import_alias(import: &ImportDecl) -> &str {
    import
        .alias
        .as_ref()
        .map(|a| a.name.as_str())
        .unwrap_or(import.path.name.as_str())
}

/// Qualified function/struct name: `math.add`.
#[must_use]
pub fn qualify(module: &str, name: &str) -> String {
    format!("{module}.{name}")
}

fn load_one(
    source: Source,
    name: String,
    from_import: Option<Span>,
    modules: &mut Vec<LoadedModule>,
    loading: &mut HashSet<String>,
) -> Result<(), ResolveError> {
    if modules.iter().any(|m| m.name == name) {
        return Ok(());
    }
    if !loading.insert(name.clone()) {
        return Err(ResolveError::new(
            from_import.unwrap_or(Span::DUMMY),
            format!("cyclic import of `{name}`"),
        ));
    }
    let program = parse(&source).map_err(|err| {
        let src = source.clone();
        ResolveError::from(err).with_source(src)
    })?;
    for item in &program.items {
        if let Item::Import(import) = item {
            let child_name = import.path.name.clone();
            let path = find_module(source.name(), &child_name).ok_or_else(|| {
                ResolveError::new(
                    import.span,
                    format!(
                        "cannot find module `{child_name}` (looked next to the importer and in `std/`)"
                    ),
                )
            })?;
            let text = fs::read_to_string(&path).map_err(|e| {
                ResolveError::new(
                    import.span,
                    format!("cannot read {}: {e}", path.display()),
                )
            })?;
            let child = Source::new(path.display().to_string(), text);
            load_one(
                child,
                child_name,
                Some(import.span),
                modules,
                loading,
            )?;
        }
    }
    loading.remove(&name);
    modules.push(LoadedModule {
        name,
        source,
        program,
    });
    Ok(())
}

fn sibling_path(importer: &str, module: &str) -> PathBuf {
    let parent = Path::new(importer).parent();
    let dir = match parent {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    dir.join(format!("{module}.flk"))
}

fn find_module(importer: &str, module: &str) -> Option<PathBuf> {
    let sibling = sibling_path(importer, module);
    if sibling.is_file() {
        return Some(sibling);
    }
    let file = format!("{module}.flk");
    let mut dir = Path::new(importer)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    loop {
        let candidate = dir.join("std").join(&file);
        if candidate.is_file() {
            return Some(candidate);
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => break,
        }
    }
    if let Ok(root) = std::env::var("FLAKE_STD") {
        let candidate = PathBuf::from(root).join(&file);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
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

    #[test]
    fn single_file_has_one_module() {
        let source = Source::new("t.flk", "fn main() { print(1) }");
        let graph = load_graph(&source).expect("load");
        assert_eq!(graph.modules.len(), 1);
        assert_eq!(graph.entry().name, "t");
    }
}
