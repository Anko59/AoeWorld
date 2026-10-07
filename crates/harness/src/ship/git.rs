//! The few Git facts `ship` needs, each from one plain `git` call.
use std::{path::Path, process::Command};

pub(crate) type Result<T> = std::result::Result<T, String>;

pub(crate) fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// The branches no agent pushes: they move only by merging a pull request.
pub(crate) fn protected(branch: &str) -> bool {
    matches!(branch, "dev" | "main") || branch.starts_with("release/")
}

/// The commit, its tree and the branch it is on; refuses what cannot ship.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Subject {
    pub(crate) head: String,
    pub(crate) tree: String,
    pub(crate) branch: String,
}

pub(crate) fn subject(root: &Path) -> Result<Subject> {
    let dirty = git(
        root,
        &["status", "--porcelain=v1", "--untracked-files=normal"],
    )?;
    if !dirty.is_empty() {
        return Err(
            "commit first: evidence is for a commit, and the tree has uncommitted or untracked changes"
                .into(),
        );
    }
    let branch = git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .map_err(|_| "HEAD is detached; ship from a feature branch".to_owned())?;
    if protected(&branch) {
        return Err(format!(
            "`{branch}` moves only by merging a pull request; ship from a feature branch"
        ));
    }
    Ok(Subject {
        head: git(root, &["rev-parse", "HEAD"])?,
        tree: git(root, &["rev-parse", "HEAD^{tree}"])?,
        branch,
    })
}

/// `refs/remotes/origin/<base>` after a fetch, never a local name that could
/// shadow it, and the merge base with HEAD.
pub(crate) fn base(root: &Path, base: &str, fetch: bool) -> Result<(String, String)> {
    if fetch {
        git(root, &["fetch", "--quiet", "origin", base])?;
    }
    let remote = git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &format!("refs/remotes/origin/{base}^{{commit}}"),
        ],
    )
    .map_err(|_| format!("origin/{base} is missing; fetch it"))?;
    let merge_base = git(root, &["merge-base", &remote, "HEAD"])?;
    Ok((remote, merge_base))
}

pub(crate) fn changed(root: &Path, merge_base: &str) -> Result<Vec<String>> {
    Ok(git(
        root,
        &[
            "diff",
            "--no-renames",
            "--name-only",
            merge_base,
            "HEAD",
            "--",
        ],
    )?
    .lines()
    .map(str::to_owned)
    .collect())
}
