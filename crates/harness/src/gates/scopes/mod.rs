//! Exact Git input scopes, not a hostile same-user sandbox.
//! Never execute candidate policy as a trusted judge merely because inputs are exact.
use serde::Serialize;
use std::{
    collections::BTreeSet,
    error::Error,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};
use tempfile::TempDir;

mod checks;
mod git;
mod index;
mod metadata;
use index::*;
use metadata::*;
mod entries;
mod export;
mod probe;
use entries::*;
use export::*;
use git::*;
use probe::*;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum Kind {
    Working,
    Index,
    /// Only a full verified object ID, never a movable ref.
    Commit(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Identity {
    pub kind: Kind,
    pub source_head: String,
    pub tree: Option<String>,
    pub index_fingerprint: Option<String>,
    pub effective_index: PathBuf,
    /// Scope contents are exactly a committed tree (NOT a trusted-policy claim).
    pub clean_commit: bool,
    pub isolated_inputs: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Probe {
    head: String,
    entries: Vec<u8>,
    fingerprint: String,
    paths: Vec<String>,
    working: Option<Vec<u8>>,
    working_clean: bool,
}

pub struct Snapshot {
    source: PathBuf,
    source_index: PathBuf,
    checkout: PathBuf,
    // Retain the owning TempDir until execution and endpoint checks finish.
    _temporary: Option<TempDir>,
    probe: Probe,
    metadata: Option<Seal>,
    pub identity: Identity,
    /// Against source HEAD. Renames contain BOTH names; deletion paths survive.
    pub paths: Vec<String>,
}

impl Snapshot {
    pub fn prepare(root: &Path, kind: Kind) -> Result<Self> {
        let index = std::env::var_os("GIT_INDEX_FILE").map(PathBuf::from);
        Self::prepare_index(root, kind, index.as_deref())
    }

    fn prepare_index(root: &Path, kind: Kind, requested_index: Option<&Path>) -> Result<Self> {
        let source = fs::canonicalize(root)?;
        // Reject a subdirectory: all recorded paths are repository-root-relative.
        let top = line(git(&source, &["rev-parse", "--show-toplevel"], None)?)?;
        if fs::canonicalize(top)? != source {
            return Err("scope root must be Git checkout root".into());
        }
        let source_index = source_index(&source, requested_index)?;
        let observed = probe(&source, &source_index, kind == Kind::Working)?;
        let (temporary, checkout, tree) = if kind == Kind::Working {
            let tree = if observed.working_clean {
                Some(resolve(&source, &observed.head, "tree")?)
            } else {
                None
            };
            (None, source.clone(), tree)
        } else {
            let (temporary, checkout, tree) =
                prepare_checkout(&source, &observed, &kind, &export_parent(&source)?)?;
            (Some(temporary), checkout, Some(tree))
        };
        if probe(&source, &source_index, kind == Kind::Working)? != observed {
            return Err("source changed while preparing snapshot; retry".into());
        }
        let head_tree = resolve(&source, &observed.head, "tree")?;
        let clean_commit = tree
            .as_ref()
            .is_some_and(|tree| matches!(kind, Kind::Commit(_)) || tree == &head_tree);
        let identity = Identity {
            kind: kind.clone(),
            source_head: observed.head.clone(),
            tree,
            index_fingerprint: Some(observed.fingerprint.clone()),
            effective_index: source_index.clone(),
            clean_commit,
            isolated_inputs: kind != Kind::Working,
        };
        let paths = if let Kind::Commit(commit) = &kind {
            names(&git(
                &source,
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-renames",
                    "--name-only",
                    "-z",
                    &observed.head,
                    commit,
                    "--",
                ],
                None,
            )?)?
        } else {
            observed.paths.clone()
        };
        let metadata = if kind == Kind::Working {
            None
        } else {
            Some(metadata_seal(&checkout, None)?)
        };
        Ok(Self {
            source,
            source_index,
            checkout,
            _temporary: temporary,
            probe: observed,
            metadata,
            identity,
            paths,
        })
    }

    pub fn root(&self) -> &Path {
        &self.checkout
    }

    /// Trusted runner supplies a typed operation; never parse a shell gate string.
    /// Source identity checks still run when the gate fails. Endpoint checks cannot
    /// detect an edit reverted between probes; Working is feedback only.
    pub fn run_checked<T>(&self, operation: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
        self.verify_source()?;
        if let Some(tree) = self
            .identity
            .tree
            .as_ref()
            .filter(|_| self.identity.isolated_inputs)
        {
            metadata_seal(self.root(), self.metadata.as_ref())?;
            verify_checkout(self.root(), tree)?;
        }
        let result = operation(self.root());
        self.verify_source()?;
        if let Some(tree) = self
            .identity
            .tree
            .as_ref()
            .filter(|_| self.identity.isolated_inputs)
        {
            metadata_seal(self.root(), self.metadata.as_ref())?;
            verify_checkout(self.root(), tree)?;
        }
        result
    }

    pub fn verify_source(&self) -> Result<()> {
        if probe(
            &self.source,
            &self.source_index,
            self.identity.kind == Kind::Working,
        )? != self.probe
        {
            return Err("source identity changed; snapshot result is stale".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

pub fn static_checks(root: &Path, kind: Kind) -> Result<()> {
    checks::run(root, kind)
}

pub fn inspect(root: &Path, revision: Option<&str>) -> Result<()> {
    let snapshot = Snapshot::prepare(
        root,
        revision.map_or(Kind::Index, |revision| Kind::Commit(revision.to_owned())),
    )?;
    snapshot.run_checked(|_| Ok(()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"identity": snapshot.identity, "paths": snapshot.paths, "authoritative": false})
        )?
    );
    Ok(())
}
