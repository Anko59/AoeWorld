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

/// The checked-out branch from its full ref: `--short` can answer
/// `heads/<name>` when a tag or remote ref shares the name.
pub(crate) fn branch(root: &Path) -> Result<String> {
    let full = git(root, &["symbolic-ref", "--quiet", "HEAD"])
        .map_err(|_| "HEAD is detached; ship from a feature branch".to_owned())?;
    full.strip_prefix("refs/heads/")
        .map(str::to_owned)
        .ok_or_else(|| format!("HEAD points at {full}, not a branch"))
}

/// `owner/name` of `origin` on GitHub, so the pull request targets the
/// repository the branch is pushed to, whatever other remotes exist.
pub(crate) fn origin_repository(root: &Path) -> Result<String> {
    let url = git(root, &["remote", "get-url", "origin"])?;
    let path = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.strip_prefix("https://github.com/"))
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))
        .ok_or_else(|| format!("origin is not a GitHub repository: {url}"))?;
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    match path.split('/').collect::<Vec<_>>().as_slice() {
        [owner, name] if !owner.is_empty() && !name.is_empty() => Ok(path.to_owned()),
        _ => Err(format!("origin is not a GitHub repository: {url}")),
    }
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
    let branch = branch(root)?;
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
