//! Affected-crate selection for the local `test-unit` (issue #388).
//!
//! The changed files map to their owning workspace crates; those crates plus
//! every crate that depends on them, transitively, are tested. A file outside
//! every crate runs the whole workspace, except under `web/`, `browser/` and
//! `docs/`, which no Rust unit test covers. Only the local preflight asks for a
//! selection (`AOE_TEST_SELECT=<merge base>`); CI and `AOE_TEST_ALL=1` never get one.
use crate::ship::git;
use serde_json::Value;
use std::{collections::BTreeSet, path::Path, process::Command};

/// Paths that hold no Rust crate and are tested by other gates.
const NO_RUST: &[&str] = &["web/", "browser/", "docs/"];

pub(crate) struct Package {
    pub(crate) name: String,
    /// Directory relative to the workspace root, without a trailing slash.
    pub(crate) dir: String,
    pub(crate) dependencies: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Selection {
    /// The whole workspace, with the reason it cannot be narrowed.
    All { total: usize, reason: String },
    Crates {
        changed: Vec<String>,
        dependents: Vec<String>,
        total: usize,
    },
}

impl Selection {
    /// The package names to test; `None` is the whole workspace.
    pub(crate) fn packages(&self) -> Option<Vec<String>> {
        match self {
            Self::All { .. } => None,
            Self::Crates {
                changed,
                dependents,
                ..
            } => Some(changed.iter().chain(dependents).cloned().collect()),
        }
    }

    pub(crate) fn line(&self) -> String {
        match self {
            Self::All { total, reason } => {
                format!("test-unit: {total} of {total} crates (all: {reason})")
            }
            Self::Crates { changed, total, .. } if changed.is_empty() => {
                format!(
                    "test-unit: 0 of {total} crates (no Rust crate changed; no Rust unit tests run)"
                )
            }
            Self::Crates {
                changed,
                dependents,
                total,
            } => format!(
                "test-unit: {} of {total} crates (changed: {}; dependents: {})",
                changed.len() + dependents.len(),
                changed.join(", "),
                if dependents.is_empty() {
                    "none".to_owned()
                } else {
                    dependents.join(", ")
                }
            ),
        }
    }
}

fn owner<'a>(packages: &'a [Package], file: &str) -> Option<&'a Package> {
    packages.iter().find(|p| {
        file.strip_prefix(&p.dir)
            .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// The dependency-graph rule and nothing else.
pub(crate) fn select(packages: &[Package], files: &[String]) -> Selection {
    let total = packages.len();
    let mut changed = BTreeSet::new();
    for file in files {
        if NO_RUST.iter().any(|prefix| file.starts_with(prefix)) {
            continue;
        }
        match owner(packages, file) {
            Some(package) => {
                changed.insert(package.name.clone());
            }
            None => {
                return Selection::All {
                    total,
                    reason: format!("{file} is outside every crate"),
                };
            }
        }
    }
    let mut affected = changed.clone();
    loop {
        let before = affected.len();
        for package in packages {
            if package.dependencies.iter().any(|d| affected.contains(d)) {
                affected.insert(package.name.clone());
            }
        }
        if affected.len() == before {
            break;
        }
    }
    Selection::Crates {
        dependents: affected.difference(&changed).cloned().collect(),
        changed: changed.into_iter().collect(),
        total,
    }
}

/// The workspace crates from `cargo metadata --format-version 1 --no-deps`.
pub(crate) fn parse_metadata(json: &str) -> Result<Vec<Package>, String> {
    let value: Value = serde_json::from_str(json).map_err(|e| format!("cargo metadata: {e}"))?;
    let root = value["workspace_root"]
        .as_str()
        .ok_or("cargo metadata: no workspace_root")?;
    let prefix = format!("{}/", root.trim_end_matches('/'));
    value["packages"]
        .as_array()
        .ok_or("cargo metadata: no packages")?
        .iter()
        .map(|package| {
            let name = package["name"].as_str().ok_or("package without a name")?;
            let manifest = package["manifest_path"]
                .as_str()
                .ok_or("no manifest_path")?;
            let dir = manifest
                .strip_prefix(&prefix)
                .and_then(|rest| rest.strip_suffix("/Cargo.toml"))
                .ok_or_else(|| format!("{name}: manifest {manifest} is outside {root}"))?;
            Ok(Package {
                name: name.to_owned(),
                dir: dir.to_owned(),
                dependencies: package["dependencies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|d| d["name"].as_str().map(str::to_owned))
                    .collect(),
            })
        })
        .collect()
}

/// Whether a selection may be made at all: never in CI, never when forced off.
pub(crate) fn allowed(get: impl Fn(&str) -> Option<String>) -> bool {
    get("GITHUB_ACTIONS").as_deref() != Some("true")
        && !get("AOE_TEST_ALL").is_some_and(|v| !v.is_empty() && v != "0")
}

pub(crate) fn real_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// The merge base the changed files are measured from, when this run may select.
pub(crate) fn requested() -> Option<String> {
    allowed(real_env)
        .then(|| real_env("AOE_TEST_SELECT"))
        .flatten()
}

/// The merge base for `make preflight`: `AOE_TEST_SELECT` when set, else the
/// one with `origin/dev`; the full suite when it cannot be found.
pub(crate) fn preflight_base() -> Option<String> {
    if !allowed(real_env) {
        return None;
    }
    if let Some(base) = real_env("AOE_TEST_SELECT") {
        return Some(base);
    }
    match git::base(Path::new("."), "dev", false) {
        Ok((_, merge_base)) => Some(merge_base),
        Err(error) => {
            eprintln!("test-unit: no merge base ({error}); running the whole workspace");
            None
        }
    }
}

/// Changed files since `base`: tracked differences in the working tree plus untracked files.
fn changed_files(root: &Path, base: &str) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    for args in [
        vec!["diff", "--no-renames", "--name-only", "-z", base, "--"],
        vec!["ls-files", "--others", "--exclude-standard", "-z"],
    ] {
        files.extend(
            git::git_raw(root, &args)?
                .split('\0')
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
        );
    }
    Ok(files)
}

pub(crate) fn plan(root: &Path, base: &str) -> Result<Selection, String> {
    let output = Command::new("cargo")
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let packages = parse_metadata(&String::from_utf8_lossy(&output.stdout))?;
    Ok(select(&packages, &changed_files(root, base)?))
}

#[cfg(test)]
mod tests;
