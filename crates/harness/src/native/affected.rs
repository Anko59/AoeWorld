//! Affected-crate selection for the local `test-unit` (issue #388).
//!
//! The changed files map to their owning workspace crates; those crates plus
//! every crate that depends on them, transitively, are tested. A file outside
//! every crate runs the whole workspace, except under `web/`, `browser/` and
//! `docs/`: those select the crates whose sources mention the path (or a parent
//! directory two levels deep or more, named alone or followed by a dynamic part, not by a
//! literal sibling), because some tests read such files. The local preflight asks for a
//! selection (`AOE_TEST_SELECT=<merge base>`); CI's `select` job makes it with the same
//! functions (`gates::select`) and hands it to the unit leg (`AOE_UNIT_SCOPE`).
//! `AOE_TEST_ALL=1` never gets one.
use crate::ship::git;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeSet, path::Path, process::Command};

/// Paths that hold no Rust crate; only the crates that name them are affected.
const NO_RUST: &[&str] = &["web/", "browser/", "docs/"];

pub(crate) struct Package {
    pub(crate) name: String,
    /// Directory relative to the workspace root, without a trailing slash.
    pub(crate) dir: String,
    pub(crate) dependencies: Vec<String>,
    /// Whether the package has a library target, which is what doctests need.
    pub(crate) lib: bool,
    /// The concatenated Rust sources of the crate, read by `plan` for path mentions.
    pub(crate) text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
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

pub(crate) fn owner<'a>(packages: &'a [Package], file: &str) -> Option<&'a Package> {
    packages.iter().find(|p| {
        file.strip_prefix(&p.dir)
            .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// The strings a crate source may use to read `file`: its path and each parent
/// directory that still has two components (`docs/geodata`, not `docs`).
pub(crate) fn mention_needles(file: &str) -> Vec<String> {
    let mut needles = vec![file.to_owned()];
    let mut path = file;
    while let Some((parent, _)) = path.rsplit_once('/') {
        if parent.contains('/') {
            needles.push(parent.to_owned());
        }
        path = parent;
    }
    needles
}

/// The Rust sources of `dir`, concatenated, skipping build output.
pub(crate) fn read_sources(dir: &Path, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "target") {
                read_sources(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            out.push_str(&text);
            out.push('\n');
        }
    }
}

/// The packages whose sources name `file`, or a parent directory of it two
/// levels deep or more: the crates whose tests may read it.
pub(crate) fn mentioning(packages: &[Package], file: &str) -> BTreeSet<String> {
    let needles = mention_needles(file);
    packages
        .iter()
        .filter(|p| {
            p.text.contains(file) || needles[1..].iter().any(|dir| names_directory(&p.text, dir))
        })
        .map(|p| p.name.clone())
        .collect()
}

/// Whether `text` names the directory `dir` itself, or a file below it by a dynamic name
/// (`dir/{name}`), and not a different, literally named sibling (`dir/other.json` is a
/// read of another file, which has its own needle).
fn names_directory(text: &str, dir: &str) -> bool {
    let continues = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '.');
    text.match_indices(dir).any(|(at, _)| {
        let rest = &text[at + dir.len()..];
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        !rest.starts_with(continues)
    })
}

/// `changed` and every package that depends on one of them, transitively.
pub(crate) fn with_dependents(
    packages: &[Package],
    changed: &BTreeSet<String>,
) -> BTreeSet<String> {
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
    affected
}

/// The dependency-graph rule, plus the path-mention rule for `NO_RUST` files.
pub(crate) fn select(packages: &[Package], files: &[String]) -> Selection {
    let total = packages.len();
    let mut changed = BTreeSet::new();
    for file in files {
        if NO_RUST.iter().any(|prefix| file.starts_with(prefix)) {
            changed.extend(mentioning(packages, file));
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
    let affected = with_dependents(packages, &changed);
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
            Ok(parse_package(package, dir))
        })
        .collect()
}

/// One package of `cargo metadata`, living in `dir` (relative to the repository root).
pub(crate) fn parse_package(package: &Value, dir: &str) -> Package {
    let lib = package["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|t| {
            t["kind"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|k| matches!(k.as_str(), Some("lib" | "rlib" | "proc-macro")))
        });
    Package {
        lib,
        text: String::new(),
        name: package["name"].as_str().unwrap_or_default().to_owned(),
        dir: dir.to_owned(),
        dependencies: package["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d["name"].as_str().map(str::to_owned))
            .collect(),
    }
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

/// `cargo metadata --no-deps` of the workspace in `root`, or of the manifest `manifest`
/// (relative to `root`) when that crate lives outside the workspace.
pub(crate) fn metadata_json(root: &Path, manifest: Option<&str>) -> Result<String, String> {
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--no-deps"]);
    match manifest {
        Some(manifest) => command.args(["--manifest-path", manifest]),
        None => command.arg("--locked"),
    };
    let output = command
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn metadata(root: &Path) -> Result<Vec<Package>, String> {
    parse_metadata(&metadata_json(root, None)?)
}

pub(crate) fn plan(root: &Path, base: &str) -> Result<Selection, String> {
    let mut packages = metadata(root)?;
    let files = changed_files(root, base)?;
    if files
        .iter()
        .any(|f| NO_RUST.iter().any(|p| f.starts_with(p)))
    {
        for package in &mut packages {
            read_sources(&root.join(&package.dir), &mut package.text);
        }
    }
    Ok(select(&packages, &files))
}

/// The selected packages that have a library target, the only ones with doctests.
pub(crate) fn library_crates(root: &Path, names: &[String]) -> Result<Vec<String>, String> {
    Ok(metadata(root)?
        .into_iter()
        .filter(|p| p.lib && names.contains(&p.name))
        .map(|p| p.name)
        .collect())
}

#[cfg(test)]
mod tests;
