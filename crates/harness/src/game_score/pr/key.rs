//! The key of a result: a hash of exactly what the `service` numbers are a function of.
//!
//! The numbers of a game are fixed by the game's code, the map and the scenarios; the machine
//! changes none of them. So a result is stored under a hash of those inputs, never under a
//! commit: two commits with the same game code share one entry, and a commit that changes only
//! a document finds the entry of its parent. The inputs are Git object ids, read from the
//! commit without a checkout: every file of every crate in the build closure of the server, the
//! lock file, the toolchain and the harness's game score module (which holds the scenarios,
//! the counters and the report schema), as `gates/ci-rules.json` lists them, less the files its
//! `ignore` globs name: the tests, benches, examples and documents of a crate are not the game,
//! and the selection already says that a change to them does not run the check.
use super::super::BoxError;
use std::{path::Path, process::Command};

const VERSION: &str = "aoe-game-score-key-v2";

/// What the numbers are a function of at one revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Inputs {
    pub map_hash: String,
    /// `(path, object id)` of every input file, sorted by path. An input that does not exist
    /// lists nothing, so a file that appears or goes changes the key all the same.
    pub objects: Vec<(String, String)>,
}

impl Inputs {
    /// The files `paths` hold at `rev` that `ignored` does not name, one `git ls-tree -r` for all
    /// of them.
    pub(super) fn read(
        root: &Path,
        rev: &str,
        map_hash: &str,
        paths: &[String],
        ignored: &dyn Fn(&str) -> bool,
    ) -> Result<Self, BoxError> {
        let output = Command::new("git")
            .current_dir(root)
            .args(["ls-tree", "-r", "-z", rev, "--"])
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
            &String::from_utf8_lossy(&output.stdout),
            ignored,
        ))
    }

    /// From the NUL-separated lines of `git ls-tree -r -z` (`mode type id<TAB>path`).
    pub(super) fn from_listing(
        map_hash: &str,
        listing: &str,
        ignored: &dyn Fn(&str) -> bool,
    ) -> Self {
        let mut objects: Vec<(String, String)> = listing
            .split('\0')
            .filter_map(|line| {
                let (meta, path) = line.split_once('\t')?;
                let id = meta.split_whitespace().nth(2)?;
                (!ignored(path)).then(|| (path.to_owned(), id.to_owned()))
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
            hash.update(id.as_bytes());
        }
        hash.finalize().to_hex().to_string()
    }
}

/// Whether the working tree differs from HEAD in any file of `paths` that `ignored` does not name
/// (a modified, deleted, renamed or new file): its result would not be the result of the commit
/// the key names.
pub(super) fn dirty(
    root: &Path,
    paths: &[String],
    ignored: &dyn Fn(&str) -> bool,
) -> Result<bool, BoxError> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["status", "--porcelain", "-z", "--untracked-files=all", "--"])
        .args(paths)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git status: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut entries = text.split('\0').filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        let (status, path) = entry.split_at(entry.len().min(3));
        // A rename or a copy is followed by the path it came from.
        let origin = status
            .bytes()
            .take(2)
            .any(|code| code == b'R' || code == b'C')
            .then(|| entries.next())
            .flatten();
        if !ignored(path) || origin.is_some_and(|origin| !ignored(origin)) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
#[path = "tests/key.rs"]
mod tests;
