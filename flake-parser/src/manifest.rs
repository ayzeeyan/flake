//! Manifest parsing and local package metadata for `flake.toml`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ManifestError {
    pub message: String,
}

impl ManifestError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// A parsed `flake.toml` manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub package: PackageInfo,
    pub dependencies: HashMap<String, Dependency>,
    pub path: PathBuf,
}

/// Metadata declared in the `[package]` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub entry: PathBuf,
    pub authors: Vec<String>,
    pub description: Option<String>,
}

/// A dependency specification in `[dependencies]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    pub path: PathBuf,
}

impl Manifest {
    /// Parse a `flake.toml` string.
    pub fn parse(content: &str, manifest_path: &Path) -> Result<Self, ManifestError> {
        let mut package_name = None;
        let mut package_version = None;
        let mut package_entry = None;
        let mut package_authors = Vec::new();
        let mut package_desc = None;
        let mut dependencies = HashMap::new();

        let mut current_section = "";
        let mut current_dep_table: Option<String> = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                let section = line[1..line.len() - 1].trim();
                if section == "package" {
                    current_section = "package";
                    current_dep_table = None;
                } else if section == "dependencies" {
                    current_section = "dependencies";
                    current_dep_table = None;
                } else if let Some(dep_name) = section.strip_prefix("dependencies.") {
                    current_section = "dep_table";
                    current_dep_table = Some(dep_name.trim().to_string());
                } else {
                    current_section = "other";
                    current_dep_table = None;
                }
                continue;
            }

            let Some((key, val)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let val = val.trim();

            match current_section {
                "package" => match key {
                    "name" => package_name = Some(unquote(val)),
                    "version" => package_version = Some(unquote(val)),
                    "entry" | "main" => package_entry = Some(PathBuf::from(unquote(val))),
                    "description" => package_desc = Some(unquote(val)),
                    "authors" => {
                        package_authors = parse_string_array(val);
                    }
                    _ => {}
                },
                "dependencies" => {
                    let dep_path = parse_dep_spec(val);
                    dependencies.insert(
                        key.to_string(),
                        Dependency {
                            name: key.to_string(),
                            path: PathBuf::from(dep_path),
                        },
                    );
                }
                "dep_table" => {
                    if let Some(ref dep_name) = current_dep_table {
                        if key == "path" {
                            let dep_path = unquote(val);
                            dependencies.insert(
                                dep_name.clone(),
                                Dependency {
                                    name: dep_name.clone(),
                                    path: PathBuf::from(dep_path),
                                },
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        let name = package_name.ok_or_else(|| {
            ManifestError::new(format!(
                "missing required field `name` in [package] section of {}",
                manifest_path.display()
            ))
        })?;

        let version = package_version.unwrap_or_else(|| "0.1.0".to_string());
        let entry = package_entry.unwrap_or_else(|| PathBuf::from("main.flk"));

        Ok(Manifest {
            package: PackageInfo {
                name,
                version,
                entry,
                authors: package_authors,
                description: package_desc,
            },
            dependencies,
            path: manifest_path.to_path_buf(),
        })
    }

    /// Discover `flake.toml` in `dir` or any of its ancestors.
    #[must_use]
    pub fn find_in_ancestors(start_dir: &Path) -> Option<(PathBuf, Self)> {
        let mut cur = if start_dir.is_file() {
            start_dir.parent().unwrap_or(Path::new("."))
        } else {
            start_dir
        };

        loop {
            let candidate = cur.join("flake.toml");
            if candidate.is_file() {
                if let Ok(content) = fs::read_to_string(&candidate) {
                    if let Ok(manifest) = Self::parse(&content, &candidate) {
                        return Some((cur.to_path_buf(), manifest));
                    }
                }
            }
            match cur.parent() {
                Some(parent) => cur = parent,
                None => break,
            }
        }
        None
    }

    /// Resolve a dependency named `dep_name` to its filesystem path relative to the manifest directory.
    #[must_use]
    pub fn resolve_dependency_path(&self, manifest_dir: &Path, dep_name: &str) -> Option<PathBuf> {
        let dep = self.dependencies.get(dep_name)?;
        let full_path = if dep.path.is_absolute() {
            dep.path.clone()
        } else {
            manifest_dir.join(&dep.path)
        };
        Some(full_path)
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
        && s.len() >= 2
    {
        return s[1..s.len() - 1].to_string();
    }
    s.to_string()
}

fn parse_string_array(s: &str) -> Vec<String> {
    let s = s.trim();
    let inner = s
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(s);
    inner
        .split(',')
        .map(|item| unquote(item.trim()))
        .filter(|item| !item.is_empty())
        .collect()
}

fn parse_dep_spec(val: &str) -> String {
    let val = val.trim();
    if val.starts_with('{') && val.ends_with('}') {
        let inner = &val[1..val.len() - 1];
        for pair in inner.split(',') {
            if let Some((k, v)) = pair.split_once('=') {
                if k.trim() == "path" {
                    return unquote(v.trim());
                }
            }
        }
    }
    unquote(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_manifest() {
        let content = r#"
[package]
name = "my_app"
version = "1.2.3"
entry = "src/main.flk"
authors = ["Alice", "Bob"]
description = "A Flake package"

[dependencies]
math = { path = "../math_lib" }
utils = "../utils_lib"

[dependencies.logger]
path = "../logger"
"#;
        let manifest = Manifest::parse(content, Path::new("flake.toml")).unwrap();
        assert_eq!(manifest.package.name, "my_app");
        assert_eq!(manifest.package.version, "1.2.3");
        assert_eq!(manifest.package.entry, PathBuf::from("src/main.flk"));
        assert_eq!(manifest.package.authors, vec!["Alice", "Bob"]);
        assert_eq!(
            manifest.package.description.as_deref(),
            Some("A Flake package")
        );

        assert_eq!(manifest.dependencies.len(), 3);
        assert_eq!(
            manifest.dependencies["math"].path,
            PathBuf::from("../math_lib")
        );
        assert_eq!(
            manifest.dependencies["utils"].path,
            PathBuf::from("../utils_lib")
        );
        assert_eq!(
            manifest.dependencies["logger"].path,
            PathBuf::from("../logger")
        );
    }
}
