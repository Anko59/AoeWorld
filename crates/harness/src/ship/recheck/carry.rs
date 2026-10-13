//! Carrying the passed review over a re-check rebase. The review that just
//! passed in this ship was judged by this run's judge and policy (the dev
//! revision `AOE_JUDGE_REV` pins), so it is not routed through the reuse check,
//! which compares against the newer `origin/dev` and would always decline.
//! It is carried on two proofs: dev's review policy is unchanged since the
//! review (its policy fingerprint at the new dev is the stored one), and the
//! own patch is identical before and after the rebase, each against its own
//! base.
use super::Result;
use crate::{
    review::Report,
    ship::{evidence::Evidence, git},
};
use sha2::Digest as _;
use std::path::Path;

/// The files whose content makes up a review's policy fingerprint.
const POLICY: [&str; 4] = [
    "gates/review.json",
    "gates/registry.json",
    "gates/review",
    "crates/harness/src/review",
];

fn short(sha: &str) -> &str {
    &sha[..12.min(sha.len())]
}

/// The dev revision the review was judged under: the pinned judge, else the
/// base the ship started from.
pub(super) fn judged_under(judge: Option<&str>, before: &Evidence) -> String {
    judge
        .map(str::trim)
        .filter(|judge| !judge.is_empty())
        .unwrap_or(&before.base)
        .to_owned()
}

/// Stops when dev's review policy differs from the one the review ran under.
pub(super) fn policy_unchanged(root: &Path, report: &Report, judged: &str) -> Result<()> {
    let dev = git::git(root, &["rev-parse", "refs/remotes/origin/dev"])?;
    let current = Report::policy_fingerprint_at(root, &dev, &report.tier)?;
    if report.policy_fingerprint.as_deref() == Some(current.as_str()) {
        return Ok(());
    }
    let mut args = vec!["rev-list", "--reverse", "--abbrev-commit", "--abbrev=12"];
    let range = format!("{judged}..{dev}");
    args.push(&range);
    args.push("--");
    args.extend(POLICY);
    let commits = git::git(root, &args).unwrap_or_default();
    let commits: Vec<&str> = commits.lines().collect();
    let named = if commits.is_empty() {
        format!("up to {}", short(&dev))
    } else {
        commits.join(" ")
    };
    Err(format!(
        "ship: the review policy on dev changed in {named}; ship again for a review under the \
         new policy. Nothing was pushed"
    )
    .into())
}

/// The own patch of HEAD against `base`, which must equal the one the review
/// covered (`before.head` against `before.base`). Returns the patch identity.
pub(super) fn patch_unchanged(root: &Path, before: &Evidence, base: &str) -> Result<Vec<u8>> {
    let old = git::change_identity_from(root, &before.base, &before.head)?.1;
    let new = git::change_identity_from(root, base, "HEAD")?.1;
    if old != new {
        return Err(format!(
            "ship: the re-check rebase changed your patch (the own change of {} differs from the \
             reviewed one of {}), so the passed review cannot be carried over; ship again to \
             review the changed part. Nothing was pushed",
            short(&git::git(root, &["rev-parse", "HEAD"])?),
            short(&before.head)
        )
        .into());
    }
    Ok(new)
}

/// The passed review as the review of the rebased head, stored so that the
/// next ship of this branch finds it. `reused_from` is the reviewed commit.
pub(super) fn carry(
    root: &Path,
    report: &Report,
    after: &Evidence,
    patch: &[u8],
) -> Result<Report> {
    let mut carried = report.clone();
    carried.reused_from = Some(std::mem::replace(&mut carried.head, after.head.clone()));
    carried.base = after.base.clone();
    carried.base_branch = after.base_branch.clone();
    carried.merge_base = after.merge_base.clone();
    carried.change_fingerprint = Some(
        sha2::Sha256::digest(patch)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    );
    carried.finished = super::super::now();
    carried.store(root)?;
    eprintln!(
        "ship: review of {} carried over the re-check rebase (same patch, same policy)",
        short(carried.reused_from.as_deref().unwrap_or_default())
    );
    Ok(carried)
}
