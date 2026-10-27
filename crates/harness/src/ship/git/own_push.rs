//! Recognize our earlier push after rebasing, without authorizing another writer.
use super::{Result, git, lease};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

#[derive(PartialEq)]
struct Patch {
    author: Vec<u8>,
    id: String,
}

/// The remote's non-merge patches must be an ordered, author-matching prefix.
pub(super) fn earlier(root: &Path, dev: &str, remote: &str, head: &str) -> Result<bool> {
    let Some(remote) = patches(root, dev, remote, true)? else {
        return Ok(false);
    };
    if remote.is_empty() {
        return Ok(false);
    }
    let Some(local) = patches(root, dev, head, false)? else {
        return Ok(false);
    };
    Ok(local.starts_with(&remote))
}

fn patches(root: &Path, dev: &str, tip: &str, remote: bool) -> Result<Option<Vec<Patch>>> {
    let commits = git(
        root,
        &[
            "rev-list",
            "--reverse",
            "--topo-order",
            "--parents",
            &format!("{dev}..{tip}"),
        ],
    )?;
    let mut patches = Vec::new();
    for line in commits.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let commit = fields[0];
        if fields.len() > 2 {
            // Local catch-up may have merged dev with the user's identity. Remote
            // merges retain the updater's existing strict parent/committer rules.
            if fields.len() != 3
                || git(root, &["merge-base", "--is-ancestor", fields[2], dev]).is_err()
                || (remote && !lease::updater_only(root, fields[1], commit, dev)?)
            {
                return Ok(None);
            }
            continue;
        }
        let author = bytes(root, &["show", "-s", "--format=%an%x00%ae", commit])?;
        let diff = bytes(
            root,
            &[
                "show",
                "--format=",
                "--patch",
                "--binary",
                "--full-index",
                "--no-renames",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                commit,
                "--",
            ],
        )?;
        let mut child = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["patch-id", "--stable"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("git patch-id: {e}"))?;
        let written = child
            .stdin
            .take()
            .ok_or("git patch-id: no stdin")?
            .write_all(&diff);
        let output = child
            .wait_with_output()
            .map_err(|e| format!("git patch-id: {e}"))?;
        written.map_err(|e| format!("git patch-id: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "git patch-id: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let id = String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned();
        // Empty commits have no stable patch ID; do not infer ownership from them.
        if id.is_empty() {
            return Ok(None);
        }
        patches.push(Patch { author, id });
    }
    Ok(Some(patches))
}

fn bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}
