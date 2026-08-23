//! Deterministic lockfile format and verification for `flake.lock`.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::manifest::{Manifest, ManifestError};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct LockfileError {
    pub message: String,
}

impl LockfileError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// A parsed or generated `flake.lock` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lockfile {
    pub version: u32,
    pub root_package: String,
    pub packages: Vec<LockedPackage>,
}

/// A locked package record in `flake.lock`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedPackage {
    pub name: String,
    pub version: String,
    pub source: String,
    pub checksum: String,
    pub dependencies: Vec<String>,
}

impl Lockfile {
    pub const CURRENT_VERSION: u32 = 1;

    /// Generate a deterministic `Lockfile` for a given `Manifest` and directory.
    pub fn generate(manifest: &Manifest, manifest_dir: &Path) -> Result<Self, LockfileError> {
        let mut packages_map: BTreeMap<String, LockedPackage> = BTreeMap::new();
        let mut visited = HashSet::new();

        // 1. Process root package
        let root_checksum = compute_package_checksum(manifest_dir)?;
        let mut root_deps: Vec<String> = manifest.dependencies.keys().cloned().collect();
        root_deps.sort();

        packages_map.insert(
            manifest.package.name.clone(),
            LockedPackage {
                name: manifest.package.name.clone(),
                version: manifest.package.version.clone(),
                source: "root".to_string(),
                checksum: root_checksum,
                dependencies: root_deps,
            },
        );
        visited.insert(manifest.package.name.clone());

        // 2. If workspace, process workspace members
        if let Some(ref ws) = manifest.workspace {
            for member_rel in &ws.members {
                let member_dir = manifest_dir.join(member_rel);
                let member_manifest_path = member_dir.join("flake.toml");
                if member_manifest_path.is_file() {
                    let content = fs::read_to_string(&member_manifest_path).map_err(|e| {
                        LockfileError::new(format!(
                            "failed to read workspace member {}: {e}",
                            member_manifest_path.display()
                        ))
                    })?;
                    let member_manifest = Manifest::parse(&content, &member_manifest_path)
                        .map_err(|e: ManifestError| LockfileError::new(e.message))?;
                    Self::collect_dependencies(
                        &member_manifest,
                        &member_dir,
                        &format!("workspace+{member_rel}"),
                        &mut packages_map,
                        &mut visited,
                    )?;
                }
            }
        }

        // 3. Process direct dependencies
        for (dep_name, dep) in &manifest.dependencies {
            let dep_dir = if dep.path.is_absolute() {
                dep.path.clone()
            } else {
                manifest_dir.join(&dep.path)
            };
            let dep_manifest_path = dep_dir.join("flake.toml");
            let source_str = format!("path+{}", dep.path.display().to_string().replace('\\', "/"));
            if dep_manifest_path.is_file() {
                let content = fs::read_to_string(&dep_manifest_path).map_err(|e| {
                    LockfileError::new(format!(
                        "failed to read dependency manifest {}: {e}",
                        dep_manifest_path.display()
                    ))
                })?;
                let dep_manifest = Manifest::parse(&content, &dep_manifest_path)
                    .map_err(|e: ManifestError| LockfileError::new(e.message))?;
                Self::collect_dependencies(
                    &dep_manifest,
                    &dep_dir,
                    &source_str,
                    &mut packages_map,
                    &mut visited,
                )?;
            } else {
                // Dependency has no flake.toml, treat directory as source package
                let checksum = compute_package_checksum(&dep_dir)?;
                let pkg_name = dep.package.as_deref().unwrap_or(dep_name);
                let version = dep.version.as_deref().unwrap_or("0.1.0");
                packages_map.entry(pkg_name.to_string()).or_insert_with(|| {
                    LockedPackage {
                        name: pkg_name.to_string(),
                        version: version.to_string(),
                        source: source_str,
                        checksum,
                        dependencies: Vec::new(),
                    }
                });
            }
        }

        let packages: Vec<LockedPackage> = packages_map.into_values().collect();

        Ok(Lockfile {
            version: Self::CURRENT_VERSION,
            root_package: manifest.package.name.clone(),
            packages,
        })
    }

    fn collect_dependencies(
        manifest: &Manifest,
        pkg_dir: &Path,
        source: &str,
        packages_map: &mut BTreeMap<String, LockedPackage>,
        visited: &mut HashSet<String>,
    ) -> Result<(), LockfileError> {
        let pkg_name = manifest.package.name.clone();
        let mut deps: Vec<String> = manifest.dependencies.keys().cloned().collect();
        deps.sort();

        let checksum = compute_package_checksum(pkg_dir)?;
        packages_map.insert(
            pkg_name.clone(),
            LockedPackage {
                name: pkg_name.clone(),
                version: manifest.package.version.clone(),
                source: source.to_string(),
                checksum,
                dependencies: deps,
            },
        );

        if !visited.insert(pkg_name) {
            return Ok(());
        }

        for (dep_name, dep) in &manifest.dependencies {
            let dep_dir = if dep.path.is_absolute() {
                dep.path.clone()
            } else {
                pkg_dir.join(&dep.path)
            };
            let dep_manifest_path = dep_dir.join("flake.toml");
            let source_str = format!("path+{}", dep.path.display().to_string().replace('\\', "/"));
            if dep_manifest_path.is_file() {
                let content = fs::read_to_string(&dep_manifest_path).map_err(|e| {
                    LockfileError::new(format!(
                        "failed to read dependency manifest {}: {e}",
                        dep_manifest_path.display()
                    ))
                })?;
                let dep_manifest = Manifest::parse(&content, &dep_manifest_path)
                    .map_err(|e: ManifestError| LockfileError::new(e.message))?;
                Self::collect_dependencies(
                    &dep_manifest,
                    &dep_dir,
                    &source_str,
                    packages_map,
                    visited,
                )?;
            } else {
                let checksum = compute_package_checksum(&dep_dir)?;
                let name = dep.package.as_deref().unwrap_or(dep_name);
                let version = dep.version.as_deref().unwrap_or("0.1.0");
                packages_map.entry(name.to_string()).or_insert_with(|| {
                    LockedPackage {
                        name: name.to_string(),
                        version: version.to_string(),
                        source: source_str,
                        checksum,
                        dependencies: Vec::new(),
                    }
                });
            }
        }

        Ok(())
    }

    /// Parse a `flake.lock` TOML string.
    pub fn parse(content: &str, lock_path: &Path) -> Result<Self, LockfileError> {
        let mut version = Self::CURRENT_VERSION;
        let mut root_package = String::new();
        let mut packages = Vec::new();

        let mut current_package: Option<LockedPackage> = None;
        let mut in_package_section = false;

        for (line_no, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line == "[[package]]" {
                if let Some(pkg) = current_package.take() {
                    packages.push(pkg);
                }
                in_package_section = true;
                current_package = Some(LockedPackage {
                    name: String::new(),
                    version: "0.1.0".to_string(),
                    source: String::new(),
                    checksum: String::new(),
                    dependencies: Vec::new(),
                });
                continue;
            }

            let Some((key, val)) = line.split_once('=') else {
                return Err(LockfileError::new(format!(
                    "syntax error at line {}: expected `key = value` in {}",
                    line_no + 1,
                    lock_path.display()
                )));
            };
            let key = key.trim();
            let val = val.trim();

            if !in_package_section {
                match key {
                    "lockfile_version" => {
                        version = val.parse::<u32>().map_err(|_| {
                            LockfileError::new(format!(
                                "invalid lockfile_version `{val}` at line {}",
                                line_no + 1
                            ))
                        })?;
                    }
                    "root_package" => {
                        root_package = unquote(val);
                    }
                    _ => {}
                }
            } else if let Some(ref mut pkg) = current_package {
                match key {
                    "name" => pkg.name = unquote(val),
                    "version" => pkg.version = unquote(val),
                    "source" => pkg.source = unquote(val),
                    "checksum" => pkg.checksum = unquote(val),
                    "dependencies" => pkg.dependencies = parse_string_array(val),
                    _ => {}
                }
            }
        }

        if let Some(pkg) = current_package {
            packages.push(pkg);
        }

        Ok(Lockfile {
            version,
            root_package,
            packages,
        })
    }

    /// Serialize the lockfile to standard TOML string.
    #[must_use]
    pub fn to_toml_string(&self) -> String {
        let mut out = String::new();
        out.push_str("# Auto-generated by Flake. Do not edit manually.\n");
        out.push_str(&format!("lockfile_version = {}\n", self.version));
        out.push_str(&format!("root_package = \"{}\"\n\n", self.root_package));

        for pkg in &self.packages {
            out.push_str("[[package]]\n");
            out.push_str(&format!("name = \"{}\"\n", pkg.name));
            out.push_str(&format!("version = \"{}\"\n", pkg.version));
            out.push_str(&format!("source = \"{}\"\n", pkg.source));
            out.push_str(&format!("checksum = \"{}\"\n", pkg.checksum));
            let deps_str = pkg
                .dependencies
                .iter()
                .map(|d| format!("\"{d}\""))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("dependencies = [{deps_str}]\n\n"));
        }

        out
    }

    /// Verify that `manifest` matches the locked packages in this lockfile.
    pub fn verify(&self, manifest: &Manifest, manifest_dir: &Path) -> Result<(), LockfileError> {
        let expected = Self::generate(manifest, manifest_dir)?;

        if self.root_package != expected.root_package {
            return Err(LockfileError::new(format!(
                "lockfile root package mismatch: locked `{}` but manifest declares `{}` (run `flake update` to refresh)",
                self.root_package, expected.root_package
            )));
        }

        let locked_by_name: BTreeMap<&str, &LockedPackage> = self
            .packages
            .iter()
            .map(|p| (p.name.as_str(), p))
            .collect();

        for expected_pkg in &expected.packages {
            match locked_by_name.get(expected_pkg.name.as_str()) {
                None => {
                    return Err(LockfileError::new(format!(
                        "package `{}` is in manifest dependencies but missing from flake.lock (run `flake lock` or `flake update`)",
                        expected_pkg.name
                    )));
                }
                Some(locked) => {
                    if locked.version != expected_pkg.version {
                        return Err(LockfileError::new(format!(
                            "version mismatch for `{}`: locked `{}` vs manifest `{}` (run `flake update`)",
                            expected_pkg.name, locked.version, expected_pkg.version
                        )));
                    }
                    if locked.source != expected_pkg.source {
                        return Err(LockfileError::new(format!(
                            "source mismatch for `{}`: locked `{}` vs current `{}` (run `flake update`)",
                            expected_pkg.name, locked.source, expected_pkg.source
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}

/// Deterministically compute an FNV-1a 64-bit hex hash over package files.
fn compute_package_checksum(dir: &Path) -> Result<String, LockfileError> {
    if !dir.is_dir() {
        return Ok("0000000000000000".to_string());
    }

    let mut files = Vec::new();
    collect_files_recursive(dir, &mut files)?;
    files.sort();

    let mut hasher = Fnv1aHasher::new();
    for file in files {
        if let Ok(rel) = file.strip_prefix(dir) {
            hasher.write(rel.to_string_lossy().as_bytes());
        }
        if let Ok(bytes) = fs::read(&file) {
            hasher.write(&bytes);
        }
    }

    Ok(format!("{:016x}", hasher.finish()))
}

fn collect_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), LockfileError> {
    let entries = fs::read_dir(dir).map_err(|e| {
        LockfileError::new(format!("failed to read directory {}: {e}", dir.display()))
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with('.') && name_str != "target" {
                collect_files_recursive(&path, out)?;
            }
        } else if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str());
            let name = path.file_name().and_then(|n| n.to_str());
            if ext == Some("flk") || name == Some("flake.toml") {
                out.push(path);
            }
        }
    }
    Ok(())
}

/// Pure-Rust FNV-1a 64-bit hasher for deterministic hashing without external dependencies.
struct Fnv1aHasher {
    state: u64,
}

impl Fnv1aHasher {
    fn new() -> Self {
        Self {
            state: 0xcbf29ce484222325,
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.state ^= byte as u64;
            self.state = self.state.wrapping_mul(0x100000001b3);
        }
    }

    fn finish(&self) -> u64 {
        self.state
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        if s.len() >= 2 {
            s[1..s.len() - 1].to_string()
        } else {
            String::new()
        }
    } else {
        s.to_string()
    }
}

fn parse_string_array(s: &str) -> Vec<String> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return Vec::new();
    }
    let inner = &s[1..s.len() - 1];
    inner
        .split(',')
        .map(|item| unquote(item.trim()))
        .filter(|item| !item.is_empty())
        .collect()
}
