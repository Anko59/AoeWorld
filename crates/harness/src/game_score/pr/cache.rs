//! The results cache: one file per key (`key`) in the shared home, holding the game score
//! report of the game state the key names. Every run of a branch stores its own entry; the merge
//! base is run only when no entry has its key.
use super::super::BoxError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Entries kept; the oldest go first. One entry is about 100 KiB.
pub(super) const KEEP: usize = 400;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(super) struct Entry {
    pub key: String,
    /// The commit whose run this is (another commit with the same key would give the same numbers).
    pub revision: String,
    /// Seconds since the epoch.
    pub stored_at: u64,
    /// The worker counts the report was played at.
    pub workers: Vec<usize>,
    /// What producing it took: building the code (merge base only) and playing the games.
    pub build_seconds: f64,
    pub play_seconds: f64,
    /// The game score report (`schema` 1) without its table.
    pub report: Value,
}

fn path(directory: &Path, key: &str) -> PathBuf {
    directory.join(format!("{key}.json"))
}

/// The entry of `key`; a file that does not read back is a miss, never an error.
pub(super) fn load(directory: &Path, key: &str) -> Option<Entry> {
    let text = fs::read_to_string(path(directory, key)).ok()?;
    let entry: Entry = serde_json::from_str(&text).ok()?;
    (entry.key == key).then_some(entry)
}

/// Writes the entry whole or not at all, so a reader on another worktree never sees half of it.
pub(super) fn store(directory: &Path, entry: &Entry) -> Result<(), BoxError> {
    fs::create_dir_all(directory)?;
    let target = path(directory, &entry.key);
    let partial = directory.join(format!("{}.{}.partial", entry.key, std::process::id()));
    fs::write(&partial, serde_json::to_vec(entry)?)?;
    fs::rename(&partial, &target)?;
    prune(directory, KEEP);
    Ok(())
}

/// Removes the oldest entries beyond `keep`, and the partial files a dead process left.
pub(super) fn prune(directory: &Path, keep: usize) {
    let Ok(listing) = fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<(SystemTime, PathBuf)> = Vec::new();
    for item in listing.flatten() {
        let item_path = item.path();
        let modified = item.metadata().and_then(|meta| meta.modified()).ok();
        match item_path.extension().and_then(|ext| ext.to_str()) {
            Some("json") => entries.extend(modified.map(|time| (time, item_path))),
            Some("partial")
                if modified
                    .and_then(|time| time.elapsed().ok())
                    .is_some_and(|age| age.as_secs() > 3600) =>
            {
                let _ = fs::remove_file(&item_path);
            }
            _ => {}
        }
    }
    entries.sort();
    let excess = entries.len().saturating_sub(keep);
    for (_, old) in entries.into_iter().take(excess) {
        let _ = fs::remove_file(old);
    }
}

/// The report as it is kept: the table is a rendering of the numbers and takes room.
pub(super) fn slim(report: &Value) -> Value {
    let mut slim = report.clone();
    if let Some(object) = slim.as_object_mut() {
        object.remove("table");
    }
    slim
}

#[cfg(test)]
#[path = "tests/cache.rs"]
mod tests;
