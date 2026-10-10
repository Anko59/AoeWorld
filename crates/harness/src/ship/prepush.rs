//! The pre-push hook re-ran `make preflight` right after `make ship` had run
//! the same gates at the same commit (docs/shipping.md). `make ship` now names
//! the commit it evidenced in AOE_SHIP_EVIDENCE; the hook skips only when that
//! commit is the clean HEAD and its evidence passed the preflight cadence.
//! CI re-runs every gate either way.
use super::{Result, evidence, git};
use std::path::Path;

pub(crate) const ENV: &str = "AOE_SHIP_EVIDENCE";

pub(crate) fn reusable(root: &Path, claimed: Option<&str>) -> Result<bool> {
    let Some(head) =
        claimed.filter(|sha| sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()))
    else {
        return Ok(false);
    };
    if git::git(root, &["rev-parse", "HEAD"])? != head
        || !git::git(root, &["status", "--porcelain"])?.is_empty()
    {
        return Ok(false);
    }
    let tree = git::git(root, &["rev-parse", "HEAD^{tree}"])?;
    Ok(evidence::read(root, head)?.is_some_and(|evidence| {
        evidence.verdict == evidence::Verdict::Pass
            && evidence.cadence == "preflight"
            && evidence.tree == tree
    }))
}

/// Succeeds when the preflight may be skipped; fails (and the hook runs
/// `make preflight`) otherwise.
pub(crate) fn check(root: &Path) -> Result<()> {
    let claimed = std::env::var(ENV).ok();
    if reusable(root, claimed.as_deref())? {
        println!("pre-push: make ship already passed the preflight gates at this commit");
        Ok(())
    } else {
        Err(
            "pre-push: no passing make ship evidence for this commit; running make preflight"
                .into(),
        )
    }
}

#[path = "prepush/tests.rs"]
#[cfg(test)]
mod tests;
