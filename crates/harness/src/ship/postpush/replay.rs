//! Diagnose GitHub's refused REBASE by replaying commits, never by merging tips.
use super::{BASE, Result, fetch};
use crate::ship::git;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

struct Worktree {
    root: PathBuf,
    path: PathBuf,
    _temp: tempfile::TempDir,
}

impl Worktree {
    fn create(root: &Path, head: &str) -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("replay");
        git::git(
            root,
            &["worktree", "add", "--detach", &path.to_string_lossy(), head],
        )?;
        Ok(Self {
            root: root.to_owned(),
            path,
            _temp: temp,
        })
    }
}

impl Drop for Worktree {
    fn drop(&mut self) {
        let _ = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["worktree", "remove", "--force"])
            .arg(&self.path)
            .output();
    }
}

/// Files of the first conflicting commit. An empty list means replay succeeded;
/// other failures stay errors so the updater can explain them in its comment.
pub(super) fn conflicting_files(
    root: &Path,
    number: u64,
    expected_head: &str,
) -> Result<Vec<String>> {
    let head = fetch(root, &format!("refs/pull/{number}/head"))?;
    if head != expected_head {
        return Err("the pull request moved during conflict diagnosis".into());
    }
    let base = fetch(root, &format!("refs/heads/{BASE}"))?;
    let fork = git::git(root, &["merge-base", &base, &head])?;
    let worktree = Worktree::create(root, &head)?;
    let replay = replay_command(&worktree.path, &base, &fork).output()?;
    if replay.status.success() {
        return Ok(Vec::new());
    }
    let unmerged = Command::new("git")
        .arg("-C")
        .arg(&worktree.path)
        .args(["diff", "--name-only", "--diff-filter=U", "-z"])
        .output()?;
    if !unmerged.status.success() {
        return Err(format!(
            "cannot read replay conflicts: {}",
            String::from_utf8_lossy(&unmerged.stderr).trim()
        )
        .into());
    }
    let files: Vec<String> = String::from_utf8(unmerged.stdout)?
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    if files.is_empty() {
        return Err(format!(
            "local commit replay failed: {}{}",
            String::from_utf8_lossy(&replay.stdout),
            String::from_utf8_lossy(&replay.stderr)
        )
        .into());
    }
    Ok(files)
}

/// The local replay of a pull request's commits onto the base. Replaying
/// creates commits, so it carries its own committer identity: a fresh CI
/// runner has none configured and would stop before the conflicting commit.
pub(super) fn replay_command(worktree: &Path, base: &str, fork: &str) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(worktree)
        .args([
            "rebase",
            "--no-autostash",
            "--no-autosquash",
            "--no-rebase-merges",
            "--onto",
            base,
            fork,
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_COMMITTER_NAME", "aoe-harness conflict check")
        .env("GIT_COMMITTER_EMAIL", "aoe-harness@localhost");
    command
}
