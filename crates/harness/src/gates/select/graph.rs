//! The crate graph a change is measured against: the workspace members plus the crates
//! that live outside the workspace (the Python bindings, the fuzz targets), which the
//! jobs build but `cargo` never lists as members.
use super::rules::Rules;
use crate::native::affected::{self, Package};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(crate) struct Graph {
    /// The workspace members first, then the crates outside the workspace.
    pub(crate) packages: Vec<Package>,
    members: usize,
    /// Normal and build dependencies by package name: what a build of the package compiles.
    build: BTreeMap<String, BTreeSet<String>>,
}

/// The build edges (every dependency except `dev`) of each package of a `cargo metadata` document.
fn build_edges(json: &str) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let value: Value = serde_json::from_str(json).map_err(|e| format!("cargo metadata: {e}"))?;
    Ok(value["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|package| {
            let name = package["name"].as_str()?.to_owned();
            let edges = package["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["kind"].as_str() != Some("dev"))
                .filter_map(|d| d["name"].as_str().map(str::to_owned))
                .collect();
            Some((name, edges))
        })
        .collect())
}

impl Graph {
    /// From the `cargo metadata` documents of the workspace and of each outside crate
    /// (with its directory, relative to the repository root).
    pub(crate) fn from_metadata(
        workspace: &str,
        outside: &[(String, String)],
    ) -> Result<Self, String> {
        let mut packages = affected::parse_metadata(workspace)?;
        let members = packages.len();
        let mut build = build_edges(workspace)?;
        for (dir, json) in outside {
            let value: Value =
                serde_json::from_str(json).map_err(|e| format!("cargo metadata of {dir}: {e}"))?;
            for package in value["packages"].as_array().into_iter().flatten() {
                packages.push(affected::parse_package(package, dir));
            }
            build.extend(build_edges(json)?);
        }
        Ok(Self {
            packages,
            members,
            build,
        })
    }

    /// Run `cargo metadata` for the workspace and for every outside crate; with `sources`,
    /// also read the Rust sources of each package, which the mention rule searches.
    pub(crate) fn load(root: &Path, rules: &Rules, sources: bool) -> Result<Self, String> {
        let workspace = affected::metadata_json(root, None)?;
        let mut outside = Vec::new();
        for dir in &rules.outside_workspace {
            let manifest = format!("{dir}/Cargo.toml");
            outside.push((dir.clone(), affected::metadata_json(root, Some(&manifest))?));
        }
        let mut graph = Self::from_metadata(&workspace, &outside)?;
        if sources {
            for package in &mut graph.packages {
                affected::read_sources(&root.join(&package.dir), &mut package.text);
            }
        }
        Ok(graph)
    }

    pub(crate) fn is_workspace(&self, name: &str) -> bool {
        self.workspace().iter().any(|p| p.name == name)
    }

    pub(crate) fn workspace(&self) -> &[Package] {
        &self.packages[..self.members]
    }

    pub(crate) fn owner(&self, file: &str) -> Option<String> {
        affected::owner(&self.packages, file).map(|p| p.name.clone())
    }

    /// The workspace crates whose sources name `file` (see [`affected::mentioning`]).
    pub(crate) fn mentioning(&self, file: &str) -> BTreeSet<String> {
        affected::mentioning(self.workspace(), file)
    }

    /// `changed` plus the workspace crates whose build or tests reach one of them.
    pub(crate) fn tested_with_dependents(&self, changed: &BTreeSet<String>) -> BTreeSet<String> {
        affected::with_dependents(self.workspace(), changed)
    }

    /// What building `subject` compiles: itself and its build dependencies, transitively.
    /// With `tests`, the subject's own dev-dependencies count too (its tests compile them).
    pub(crate) fn closure(&self, subject: &str, tests: bool) -> BTreeSet<String> {
        let mut result = BTreeSet::from([subject.to_owned()]);
        let mut pending = vec![subject.to_owned()];
        if tests {
            let all = self
                .packages
                .iter()
                .find(|p| p.name == subject)
                .map(|p| p.dependencies.clone())
                .unwrap_or_default();
            for name in all {
                if result.insert(name.clone()) {
                    pending.push(name);
                }
            }
        }
        while let Some(next) = pending.pop() {
            for name in self.build.get(&next).into_iter().flatten() {
                if result.insert(name.clone()) {
                    pending.push(name.clone());
                }
            }
        }
        result
    }
}
