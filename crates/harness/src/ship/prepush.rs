//! The pre-push hook re-ran `make preflight` right after `make ship` had run
//! the same gates at the same commit (docs/shipping.md). `make ship` now names
//! the commit it evidenced in AOE_SHIP_EVIDENCE; the hook skips only when that
//! commit is the clean HEAD and its evidence passed the preflight cadence.
//! A head rebased by the re-check before the push has no preflight of its own:
//! it is accepted only through its explicit re-check record (static gates
//! passed on this head, full preflight passed on the recorded pre-rebase head
//! with an identical own patch). CI re-runs every gate either way.
use super::{
    Result,
    evidence::{self, Evidence},
    git,
    recheck::Recheck,
    run::GateVerdict,
};
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
            && evidence.tree == tree
            && match &evidence.recheck {
                None => evidence.cadence == "preflight",
                Some(recheck) => rechecked(root, &evidence, recheck).unwrap_or(false),
            }
    }))
}

/// The re-check record of a rebased head: its static gates passed on this very
/// head, and the full preflight it names passed on another commit of the same
/// branch whose own patch (against its own base) is identical.
fn rechecked(root: &Path, evidence: &Evidence, recheck: &Recheck) -> Result<bool> {
    let static_passed = evidence.cadence == "recheck"
        && !evidence.gates.is_empty()
        && evidence
            .gates
            .iter()
            .all(|gate| gate.verdict == GateVerdict::Pass)
        && recheck.head == evidence.head
        && recheck.base == evidence.base;
    let Some(from) = evidence::read(root, &recheck.from_head)? else {
        return Ok(false);
    };
    if !static_passed
        || from.verdict != evidence::Verdict::Pass
        || from.cadence != "preflight"
        || from.recheck.is_some()
        || from.head != recheck.from_head
        || from.base != recheck.from_base
        || from.branch != evidence.branch
    {
        return Ok(false);
    }
    let before = git::change_identity_from(root, &from.base, &from.head)?.1;
    let after = git::change_identity_from(root, &evidence.base, &evidence.head)?.1;
    Ok(before == after)
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
