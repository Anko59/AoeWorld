//! A tester round kept after an unavailable check: recorded by the driver, resumed only on an exact match.
use crate::ship::{duel::tree::changed, git};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path, path::PathBuf};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Kept {
    branch: String,
    head: String,
    /// Kept path to the Git object hash of its content (`deleted` when removed).
    files: BTreeMap<String, String>,
}

fn marker(root: &Path, branch: &str) -> git::Result<PathBuf> {
    let common = git::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    Ok(PathBuf::from(common)
        .join("aoe-ship/duel-kept")
        .join(format!("{}.json", branch.replace(['/', '\\'], "_"))))
}

fn snapshot(root: &Path, branch: &str, paths: &[String]) -> git::Result<Kept> {
    let mut files = BTreeMap::new();
    for path in paths {
        let hash = if root.join(path).is_file() {
            git::git(root, &["hash-object", "--", path])?
        } else {
            "deleted".to_owned()
        };
        files.insert(path.clone(), hash);
    }
    Ok(Kept {
        branch: branch.to_owned(),
        head: git::git(root, &["rev-parse", "HEAD"])?,
        files,
    })
}

/// Record the test changes in the working tree as kept by this driver.
pub(crate) fn write(root: &Path, branch: &str) -> git::Result<()> {
    let kept = snapshot(root, branch, &changed(root)?)?;
    let path = marker(root, branch)?;
    fs::create_dir_all(path.parent().ok_or("marker has no parent")?).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&kept).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}

/// Forget the marker: the round committed or the duel ended otherwise.
pub(crate) fn remove(root: &Path, branch: &str) -> git::Result<()> {
    match fs::remove_file(marker(root, branch)?) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

/// Whether a dirty tree is exactly the round this driver kept. `Ok(false)` means there is
/// no record (the caller refuses a dirty tree); an error names how a record differs.
pub(crate) fn matches(root: &Path, branch: &str) -> git::Result<bool> {
    let paths = changed(root)?;
    let text = match fs::read_to_string(marker(root, branch)?) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.to_string()),
    };
    if paths.is_empty() {
        remove(root, branch)?; // Committed, stashed or discarded since: the record is stale.
        return Ok(false);
    }
    let recorded: Kept = serde_json::from_str(&text).map_err(|e| format!("kept record: {e}"))?;
    let now = snapshot(root, branch, &paths)?;
    if recorded == now {
        return Ok(true);
    }
    let mut differences = Vec::new();
    if recorded.head != now.head {
        differences.push(format!("HEAD moved from {}", recorded.head));
    }
    for (path, hash) in &now.files {
        match recorded.files.get(path) {
            None => differences.push(format!("{path} was not kept")),
            Some(kept) if kept != hash => differences.push(format!("{path} changed since")),
            _ => {}
        }
    }
    for path in recorded
        .files
        .keys()
        .filter(|p| !now.files.contains_key(*p))
    {
        differences.push(format!("{path} is no longer changed"));
    }
    Err(format!(
        "commit or stash the changes: the tree is not the tester round this duel kept ({})",
        differences.join(", ")
    ))
}
