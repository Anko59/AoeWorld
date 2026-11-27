//! The key of a result: a hash of exactly what the `service` numbers are a function of.
//!
//! The numbers of a game are fixed by the game's code, the map and the scenarios; the machine
//! changes none of them. So a result is stored under a hash of those inputs, never under a
//! commit: two commits with the same game code share one entry, and a commit that changes only
//! a document finds the entry of its parent. The inputs are Git object ids, read from the
//! commit without a checkout: the tree of every crate in the build closure of the server, the
//! lock file, the toolchain and the harness's game score module (which holds the scenarios,
//! the counters and the report schema), as `gates/ci-rules.json` lists them.
use super::super::BoxError;
use std::{path::Path, process::Command};

const VERSION: &str = "aoe-game-score-key-v1";

/// The Git object id of each input at one revision, `None` where the input does not exist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Inputs {
    pub map_hash: String,
    /// `(path, object id)`, sorted by path.
    pub objects: Vec<(String, Option<String>)>,
}

impl Inputs {
    /// The inputs `paths` hold at `rev`, one `git ls-tree` for all of them.
    pub(super) fn read(
        root: &Path,
        rev: &str,
        map_hash: &str,
        paths: &[String],
    ) -> Result<Self, BoxError> {
        let output = Command::new("git")
            .current_dir(root)
            .args(["ls-tree", "-z", rev, "--"])
            .args(paths)
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "git ls-tree {rev}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )
            .into());
        }
        Ok(Self::from_listing(
            map_hash,
            paths,
            &String::from_utf8_lossy(&output.stdout),
        ))
    }

    /// From the NUL-separated lines of `git ls-tree -z` (`mode type id<TAB>path`).
    pub(super) fn from_listing(map_hash: &str, paths: &[String], listing: &str) -> Self {
        let found: Vec<(&str, &str)> = listing
            .split('\0')
            .filter_map(|line| {
                let (meta, path) = line.split_once('\t')?;
                Some((path, meta.split_whitespace().nth(2)?))
            })
            .collect();
        let mut objects: Vec<(String, Option<String>)> = paths
            .iter()
            .map(|path| {
                let id = found
                    .iter()
                    .find(|(found, _)| found == path)
                    .map(|(_, id)| (*id).to_owned());
                (path.clone(), id)
            })
            .collect();
        objects.sort();
        objects.dedup();
        Self {
            map_hash: map_hash.to_owned(),
            objects,
        }
    }

    /// Sixty-four hex digits.
    pub(super) fn key(&self) -> String {
        let mut hash = blake3::Hasher::new();
        hash.update(VERSION.as_bytes());
        hash.update(b"\0schema=");
        hash.update(super::super::report::SCHEMA.to_string().as_bytes());
        hash.update(b"\0map=");
        hash.update(self.map_hash.as_bytes());
        for (path, id) in &self.objects {
            hash.update(b"\0");
            hash.update(path.as_bytes());
            hash.update(b"=");
            hash.update(id.as_deref().unwrap_or("absent").as_bytes());
        }
        hash.finalize().to_hex().to_string()
    }
}

/// Whether the working tree differs from HEAD in any of `paths` (a modified, deleted or new
/// file): its result would not be the result of the commit the key names.
pub(super) fn dirty(root: &Path, paths: &[String]) -> Result<bool, BoxError> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["status", "--porcelain", "--"])
        .args(paths)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git status: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(!output.stdout.is_empty())
}

#[cfg(test)]
#[path = "tests/key.rs"]
mod tests;
