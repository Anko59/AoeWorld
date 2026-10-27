//! Refresh a push lease across GitHub dev merges or our own earlier pushes.
use super::{Result, git};
use std::path::Path;

pub(in crate::ship) fn tracking(root: &Path, branch: &str) -> Option<String> {
    git(
        root,
        &[
            "rev-parse",
            "--verify",
            &format!("refs/remotes/origin/{branch}^{{commit}}"),
        ],
    )
    .ok()
}

/// Walk the branch's first parents, excluding the dev history brought in by each merge.
/// Every step must have exactly two parents, GitHub's committer, and a dev second parent.
pub(in crate::ship) fn updater_only(
    root: &Path,
    started: &str,
    remote: &str,
    dev: &str,
) -> Result<bool> {
    let mut tip = remote.to_owned();
    while tip != started {
        let commit = git(root, &["show", "-s", "--format=%cn%n%ce%n%P", &tip])?;
        let fields: Vec<_> = commit.lines().collect();
        if fields.len() != 3 || fields[0] != "GitHub" || fields[1] != "noreply@github.com" {
            return Ok(false);
        }
        let parents: Vec<_> = fields[2].split_whitespace().collect();
        if parents.len() != 2
            || git(root, &["merge-base", "--is-ancestor", parents[1], dev]).is_err()
        {
            return Ok(false);
        }
        tip = parents[0].to_owned();
    }
    Ok(true)
}

pub(in crate::ship) fn refresh(
    root: &Path,
    branch: &str,
    expected: &str,
    started: &str,
    head: &str,
) -> Result<String> {
    refresh_with(root, branch, expected, started, head, false)
}

/// Without force, permit fast-forwards, updater merges, and our earlier patches.
pub(in crate::ship) fn refresh_default(
    root: &Path,
    branch: &str,
    started: &str,
    head: &str,
    existed: bool,
) -> Result<String> {
    refresh_with(
        root,
        branch,
        if existed { started } else { "" },
        started,
        head,
        true,
    )
}

fn refresh_with(
    root: &Path,
    branch: &str,
    expected: &str,
    started: &str,
    head: &str,
    fast_forward: bool,
) -> Result<String> {
    let reference = format!("refs/heads/{branch}");
    if git(root, &["ls-remote", "origin", &reference])?.is_empty() {
        // Keep the original lease: a deletion must not silently authorize recreation.
        return Ok(expected.to_owned());
    }
    git(
        root,
        &[
            "fetch",
            "--quiet",
            "--no-tags",
            "--refmap=",
            "origin",
            &reference,
        ],
    )?;
    let remote = git(root, &["rev-parse", "FETCH_HEAD^{commit}"])?;
    if remote == expected || remote == started {
        return Ok(remote);
    }
    if fast_forward && git(root, &["merge-base", "--is-ancestor", &remote, head]).is_ok() {
        return Ok(remote);
    }
    git(
        root,
        &[
            "fetch",
            "--quiet",
            "--no-tags",
            "--refmap=",
            "origin",
            "refs/heads/dev",
        ],
    )?;
    let dev = git(root, &["rev-parse", "FETCH_HEAD^{commit}"])?;
    if remote != started && updater_only(root, started, &remote, &dev)? {
        eprintln!(
            "ship: origin/{branch} gained only GitHub merges of dev; leasing on {}",
            &remote[..12]
        );
        return Ok(remote);
    }
    // The checkout can advance while this ship waits for the lock. Only the
    // evidenced commit being pushed may authorize replacing an earlier push.
    if super::own_push::earlier(root, &dev, &remote, head)? {
        eprintln!("ship: origin/{branch} held an earlier push of the same patch; replacing it");
        return Ok(remote);
    }
    Err(format!(
        "ship: origin/{branch} changed beyond the expected head; not solely GitHub merges of dev since this ship started; refusing push"
    ))
}
