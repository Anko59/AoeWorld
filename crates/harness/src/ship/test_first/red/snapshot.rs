//! The red tree: the merge base with the PR's test files laid over it.
use super::git;
use std::{fs, path::Path, process::Command};

/// Copy each of `paths` as it is at `head` into `target`; remove those `head` deleted.
pub(super) fn overlay(root: &Path, head: &str, target: &Path, paths: &[String]) -> git::Result<()> {
    for path in paths {
        let destination = target.join(path);
        let output = Command::new("git")
            .current_dir(root)
            .args(["cat-file", "blob", &format!("{head}:{path}")])
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&destination, output.stdout).map_err(|e| e.to_string())?;
        } else if destination.is_file() {
            fs::remove_file(&destination).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
