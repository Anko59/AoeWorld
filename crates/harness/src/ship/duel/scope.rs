//! Focused package and browser selection; unsupported paths require a main-session override.
use crate::ship::git;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Scope {
    pub(crate) packages: Vec<String>,
    pub(crate) specs: Vec<String>,
    pub(crate) paths: Vec<String>,
    pub(crate) manifests: Vec<String>,
}
impl Scope {
    pub(crate) fn from_paths(root: &Path, paths: &[String]) -> git::Result<Self> {
        let mut packages = BTreeSet::new();
        let mut manifests = BTreeSet::new();
        let mut specs = BTreeSet::new();
        let mut unmapped = None;
        'paths: for path in paths {
            // Fixtures and testdata ride on their package's check when one owns them.
            let lenient = !path.ends_with(".rs");
            let unsupported = || {
                format!(
                    "unsupported focused test path: {path}; set DUEL_CHECK (for example `make fuzz-smoke` for fuzz targets)"
                )
            };
            if path.starts_with("fuzz/") {
                return Err(unsupported());
            }
            if path.ends_with(".spec.ts") || path.ends_with(".test.ts") {
                if !path.starts_with("browser/") {
                    return Err(format!("spec outside browser: {path}"));
                }
                specs.insert(path.clone());
                continue;
            }
            if path.starts_with("browser/tests/") {
                browser_specs(root, &root.join("browser/tests"), &mut specs)?;
                continue;
            }
            if !crate::agents::is_test_path(path) {
                return Err(unsupported());
            }
            let mut owner = root.join(path);
            let mut found = false;
            while owner.pop() && owner.starts_with(root) {
                let manifest = owner.join("Cargo.toml");
                if !manifest.is_file() {
                    continue;
                }
                let value: toml::Value =
                    toml::from_str(&fs::read_to_string(&manifest).map_err(|e| e.to_string())?)
                        .map_err(|e| format!("{e}"))?;
                if let Some(name) = value
                    .get("package")
                    .and_then(|p| p.get("name"))
                    .and_then(|v| v.as_str())
                {
                    let workspace: toml::Value = toml::from_str(
                        &fs::read_to_string(root.join("Cargo.toml")).map_err(|_| unsupported())?,
                    )
                    .map_err(|_| unsupported())?;
                    let relative = owner
                        .strip_prefix(root)
                        .map_err(|e| e.to_string())?
                        .to_string_lossy();
                    if value.get("workspace").is_some() && owner != root
                        || !workspace
                            .get("workspace")
                            .and_then(|w| w.get("members"))
                            .and_then(|m| m.as_array())
                            .is_some_and(|members| {
                                members
                                    .iter()
                                    .any(|m| m.as_str() == Some(relative.as_ref()))
                            })
                    {
                        if lenient {
                            unmapped.get_or_insert(unsupported());
                            continue 'paths;
                        }
                        return Err(unsupported());
                    }
                    packages.insert(name.to_owned());
                    manifests.insert(
                        manifest
                            .strip_prefix(root)
                            .map_err(|e| e.to_string())?
                            .to_string_lossy()
                            .into_owned(),
                    );
                    found = true;
                    break;
                }
            }
            if !found {
                if lenient {
                    unmapped.get_or_insert(unsupported());
                    continue;
                }
                return Err(unsupported());
            }
        }
        if packages.is_empty() && specs.is_empty() {
            return Err(unmapped.unwrap_or_else(|| "no focused tests selected".into()));
        }
        Ok(Self {
            packages: packages.into_iter().collect(),
            manifests: manifests.into_iter().collect(),
            specs: specs.into_iter().collect(),
            paths: paths.to_vec(),
        })
    }
    pub(crate) fn cargo_args(&self) -> Vec<String> {
        let mut args: Vec<String> = [
            "nextest",
            "run",
            "--locked",
            "--no-fail-fast",
            "--message-format",
            "libtest-json",
            "--message-format-version",
            "0.1",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        for package in &self.packages {
            args.extend(["-p".into(), package.clone()]);
        }
        args
    }
}

fn browser_specs(root: &Path, directory: &Path, specs: &mut BTreeSet<String>) -> git::Result<()> {
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            browser_specs(root, &path, specs)?;
        } else if path
            .to_str()
            .is_some_and(|s| s.ends_with(".spec.ts") || s.ends_with(".test.ts"))
        {
            specs.insert(
                path.strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    Ok(())
}
