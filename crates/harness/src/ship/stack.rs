//! Stacked pull requests (`make ship SHIP_BASE=<branch>`): a branch that
//! depends on an unmerged one is gated, reviewed and opened against it, so
//! both are reviewed and tested at once. The base must be `dev` or a branch
//! whose open pull request `make ship` opened; auto-merge waits until the
//! parent merged and the branch was restacked onto dev.
use super::{Result, git, github::gh};
use crate::review::base::DEV;
use serde::Deserialize;
use std::path::Path;

/// One pull request whose head is the requested base, as `gh pr list` lists it.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct HeadPr {
    pub(crate) number: u64,
    pub(crate) state: String,
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) body: String,
    #[serde(rename = "isCrossRepository")]
    pub(crate) cross_repository: bool,
}

/// The open pull request a stacked branch is reviewed and opened against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Parent {
    pub(crate) number: u64,
    pub(crate) branch: String,
    pub(crate) url: String,
}

/// Refuse a base that is not `dev` and cannot be a parent branch, before
/// anything runs: another protected branch, the branch itself or a name Git
/// or `gh` could read as something else.
pub(crate) fn refuse_base(base: &str, branch: &str) -> std::result::Result<(), String> {
    if base == DEV {
        return Ok(());
    }
    let plain = !base.is_empty()
        && !base.starts_with(['-', '/', '.'])
        && !base.ends_with(['/', '.'])
        && !base.ends_with(".lock")
        && !base.contains("..")
        && !base.contains("//")
        && base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'));
    if !plain {
        return Err(format!("SHIP_BASE={base:?} is not a plain branch name"));
    }
    if git::protected(base) {
        return Err(format!(
            "SHIP_BASE={base}: a pull request goes into `dev` or stacks on an open harness pull request, never into `{base}`"
        ));
    }
    if base == branch {
        return Err(format!(
            "SHIP_BASE={base}: a branch cannot be stacked on itself"
        ));
    }
    Ok(())
}

/// Body markers of a description rendered by `make ship`.
fn opened_by_the_harness(body: &str) -> bool {
    body.contains("# 🤖 For AI") && body.contains("## Adversarial review")
}

/// The open harness pull request whose head is `base`, or why there is none.
pub(crate) fn parent_from(base: &str, prs: &[HeadPr]) -> std::result::Result<Parent, String> {
    let ours: Vec<&HeadPr> = prs.iter().filter(|pr| !pr.cross_repository).collect();
    if let Some(open) = ours.iter().find(|pr| pr.state == "OPEN") {
        if !opened_by_the_harness(&open.body) {
            return Err(format!(
                "SHIP_BASE={base}: #{} was not opened by `make ship`; stack only on a harness pull request",
                open.number
            ));
        }
        return Ok(Parent {
            number: open.number,
            branch: base.to_owned(),
            url: open.url.clone(),
        });
    }
    let restack = "rebase onto origin/dev instead (`git rebase --onto origin/dev <old parent tip>`) and ship with SHIP_BASE=dev SHIP_FORCE=1";
    if let Some(merged) = ours.iter().find(|pr| pr.state == "MERGED") {
        return Err(format!(
            "SHIP_BASE={base}: #{} is already merged; {restack}",
            merged.number
        ));
    }
    if let Some(closed) = ours.iter().find(|pr| pr.state == "CLOSED") {
        return Err(format!(
            "SHIP_BASE={base}: #{} is closed; {restack}",
            closed.number
        ));
    }
    Err(format!(
        "SHIP_BASE={base}: origin has no pull request from `{base}`; stack only on a branch shipped with `make ship`"
    ))
}

/// Look the parent up on origin's repository by head branch.
pub(crate) fn parent(root: &Path, repository: &str, base: &str) -> Result<Parent> {
    let listed = gh(
        root,
        &[
            "pr",
            "list",
            "--repo",
            repository,
            "--head",
            base,
            "--state",
            "all",
            "--json",
            "number,state,url,body,isCrossRepository",
        ],
    )?;
    let prs: Vec<HeadPr> = serde_json::from_str(&listed)?;
    Ok(parent_from(base, &prs)?)
}

/// The base to move an open pull request to, when it targets another one
/// (a restack onto dev after the parent merged).
pub(crate) fn retarget<'a>(existing: Option<&str>, base: &'a str) -> Option<&'a str> {
    existing.filter(|current| *current != base).map(|_| base)
}

/// The line at the top of a stacked pull request's description.
pub(crate) fn note(parent: &Parent) -> String {
    format!(
        "> [!NOTE]\n> Stacked on #{} (`{}`): gated and reviewed against it, so this diff holds only this change. Auto-merge is armed only once #{} has merged and this branch is restacked onto `dev` (`make ship SHIP_BASE=dev`).\n",
        parent.number, parent.branch, parent.number
    )
}

/// What happens next, printed after the pull request is published.
pub(crate) fn next_steps(parent: Option<&Parent>, url: &str) -> String {
    match parent {
        Some(parent) => format!(
            "ship: {url} is stacked on #{n} and reviewed against `{b}`; CI runs on it now, and auto-merge is not armed. After #{n} merges: `git fetch origin && git rebase --onto origin/dev <old {b} tip>`, commit nothing else, then `make ship SHIP_BASE=dev`: the identical change reuses this review, the pull request is retargeted to dev, force-pushed with lease and auto-merge is armed.",
            n = parent.number,
            b = parent.branch
        ),
        None => format!(
            "ship: {url} targets dev; auto-merge is armed and GitHub merges once the required checks pass."
        ),
    }
}
