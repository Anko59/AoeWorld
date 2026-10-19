//! The fallback when a rebase onto the moved base conflicts: merge the base instead.
use super::{Result, git, short};
use std::path::Path;

/// Merge `latest` into HEAD. Returns true when the merge is clean and committed (with the
/// user's identity and signing configuration, like any local commit). A conflicting merge
/// is aborted; the branch must then be back at `round_head`, or this is an error.
pub(super) fn clean(root: &Path, latest: &str, round_head: &str) -> Result<bool> {
    let message = format!("Merge {} into the branch", short(latest));
    if git::git(
        root,
        &["merge", "--quiet", "--no-edit", "-m", &message, latest],
    )
    .is_ok()
    {
        return Ok(true);
    }
    let aborted = git::git(root, &["merge", "--abort"]);
    let merging = git::git(root, &["rev-parse", "--git-path", "MERGE_HEAD"])
        .is_ok_and(|path| root.join(path).exists());
    let restored = git::git(root, &["rev-parse", "HEAD"]).is_ok_and(|head| head == round_head);
    if merging || !restored {
        return Err(format!(
            "ship: the merge fallback failed and the abort did not restore {} ({}): run \
             `git merge --abort` and check the branch. Nothing was pushed",
            short(round_head),
            aborted.err().unwrap_or_else(|| "abort succeeded".into())
        )
        .into());
    }
    Ok(false)
}

/// Whether a merge may stand in for the rebase: only when the rebase's lower bound is the
/// natural merge base of the branch and the new base, as when dev simply moved ahead. A
/// rewritten stacked parent puts the recorded fork point elsewhere; a merge would then use an
/// older base and bring back parent changes the rewrite removed, so the conflict stops.
pub(super) fn allowed(root: &Path, fork_point: &str, latest: &str, head: &str) -> bool {
    let natural = git::git(root, &["merge-base", head, latest]);
    let fork = git::git(
        root,
        &["rev-parse", "--verify", &format!("{fork_point}^{{commit}}")],
    );
    matches!((natural, fork), (Ok(natural), Ok(fork)) if natural == fork)
}
