//! The re-check just before the push (docs/shipping.md). The gates and the
//! review take minutes to an hour, and the base can move meanwhile: two
//! branches that each pass alone can break together (a directory pushed past
//! its file limit, a test file past its line limit, a conflict). Holding the
//! ship lock, after the review passed, `make ship` fetches the base again.
//! If it moved, the branch is rebased onto it and the whole-repository
//! `static` preflight gates run on the result. The passed review stays valid
//! by the patch-identity rules; it is never run again here.
use super::{
    Options,
    evidence::{self, Evidence},
    git,
    run::{self, GateResult, GateVerdict},
};
use crate::{
    gates::registry::{Cadence, Gate, Registry},
    review::{Report, base},
    slots::SlotEnv,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

mod carry;
mod merge;
pub(super) mod start;

#[cfg(test)]
mod tests;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// How often the base may move again during the re-check before the ship stops.
pub(super) const MAX_ROUNDS: u32 = 3;

/// What the re-check did, kept in the evidence and the pull request description.
/// The evidence's own `gates` are the static gates that ran on the rebased head
/// (cadence `recheck`); the full preflight is only referenced here, by the
/// commit and base it passed on. The pre-push hook accepts the rebased head
/// through this record alone (`prepush.rs`).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Recheck {
    /// The rebased head the static gates ran on.
    pub(crate) head: String,
    /// The base commit the branch was rebased onto.
    pub(crate) base: String,
    /// The commit the full preflight passed on (before the rebase) ...
    pub(crate) from_head: String,
    /// ... and the base it was run against.
    pub(crate) from_base: String,
    /// The full preflight gates that passed on `from_head`.
    pub(crate) preflight: Vec<GateResult>,
    /// The dev revision the carried review was judged under, if one was.
    pub(crate) judge: Option<String>,
    pub(crate) rounds: u32,
    /// The commits the base gained meanwhile, oldest first.
    pub(crate) commits: Vec<String>,
}

/// The branch after a re-check rebase: evidence and review for the new commit.
pub(super) struct Rebased {
    pub(super) evidence: Evidence,
    pub(super) review: Option<Report>,
    /// The commit the remote branch may be overwritten from, when it is the
    /// one this ship started from (`--force-with-lease=<ref>:<commit>`).
    pub(super) lease: Option<String>,
    /// Keeps the new base pinned for the description and the pull request.
    pub(super) _anchor: base::Scope,
}

fn short(sha: &str) -> &str {
    &sha[..12.min(sha.len())]
}

/// Fetch the base again; `None` when nothing moved. A local parent cannot
/// move under the ship. `review` is the review that just passed for
/// `evidence.head`, carried over a rebase that leaves the patch unchanged.
/// `started_head` is the draft-pushed head when CI started before review;
/// otherwise it is the checkout's head at the start of ship.
pub(super) fn before_push(
    root: &Path,
    evidence: &Evidence,
    review: Option<&Report>,
    options: &Options,
    started_head: &str,
) -> Result<Option<Rebased>> {
    let judge = std::env::var("AOE_JUDGE_REV").ok();
    before_push_from(
        root,
        evidence,
        review,
        options,
        judge.as_deref(),
        started_head,
    )
}

fn before_push_from(
    root: &Path,
    evidence: &Evidence,
    review: Option<&Report>,
    options: &Options,
    judge: Option<&str>,
    started_head: &str,
) -> Result<Option<Rebased>> {
    if base::is_local(&options.base) {
        return Ok(None);
    }
    let from_head = evidence.head.clone();
    let started = super::now();
    let judged = carry::judged_under(judge, evidence);
    let mut current = evidence.base.clone();
    let mut merge_base = evidence.merge_base.clone();
    let (mut rounds, mut commits, mut results) = (0, Vec::new(), Vec::new());
    loop {
        let latest = fetch(root, &options.base, &current)?;
        if options.parent.is_some() {
            let repository = git::origin_repository(root)?;
            let parent = super::stack::parent(root, &repository, &options.base)
                .map_err(|error| format!("{error}\nPinned parent tip: {current}"))?;
            if parent.is_none() {
                return Err(restack(
                    &options.base,
                    &current,
                    "the parent pull request disappeared",
                )
                .into());
            }
        }
        if latest == current {
            break;
        }
        if rounds == MAX_ROUNDS {
            return Err(format!(
                "ship: {} kept moving: {MAX_ROUNDS} re-checks were not enough to catch up; \
                 nothing was pushed. Ship again when it is quieter",
                options.base
            )
            .into());
        }
        rounds += 1;
        let gained = gained(root, &current, &latest)?;
        let round_head = git::git(root, &["rev-parse", "HEAD"])?;
        rebase(
            root,
            options,
            &latest,
            &merge_base,
            &round_head,
            "moved during the ship",
        )?;
        eprintln!(
            "ship: {} moved by {} commit(s) during the ship; rebased onto {}, re-checking",
            options.base,
            gained.len(),
            short(&latest)
        );
        if let Some(report) = review {
            carry::policy_unchanged(root, report, &judged)?;
            carry::patch_unchanged(root, evidence, &latest)?;
        }
        results = static_gates(root, &options.slots)?;
        if let Some(failed) = results.iter().find(|r| r.verdict != GateVerdict::Pass) {
            return Err(failure(options, failed, &gained).into());
        }
        merge_base = git::git(root, &["merge-base", &latest, "HEAD"])?;
        commits.extend(gained);
        current = latest;
    }
    if rounds == 0 {
        return Ok(None);
    }
    let subject = git::subject(root)?;
    let lease = lease(root, options, &subject.branch, &subject.head, started_head)?;
    let anchor = base::scope_at(&options.base, &current);
    let after = Evidence {
        cadence: "recheck".into(),
        verdict: evidence::verdict(&results),
        head: subject.head.clone(),
        tree: subject.tree,
        base: current.clone(),
        changed: git::changed(root, &merge_base)?,
        merge_base,
        gates: results,
        recheck: Some(Recheck {
            head: subject.head,
            base: current,
            from_head,
            from_base: evidence.base.clone(),
            preflight: evidence.gates.clone(),
            judge: judge.map(str::to_owned),
            rounds,
            commits,
        }),
        started,
        finished: super::now(),
        ..evidence.clone()
    };
    let path = evidence::write(root, &after)?;
    eprintln!("ship: evidence {}", path.display());
    let review = match review {
        Some(report) if !options.no_review => {
            let patch = carry::patch_unchanged(root, evidence, &after.base)?;
            Some(carry::carry(root, report, &after, &patch)?)
        }
        _ => None,
    };
    Ok(Some(Rebased {
        evidence: after,
        review,
        lease,
        _anchor: anchor,
    }))
}

fn restack(base: &str, tip: &str, reason: &str) -> String {
    format!(
        "ship: {base}: {reason}; restack with `git rebase --onto origin/dev {tip}` \
             then ship with SHIP_BASE=dev SHIP_FORCE=1. The passed review is retained; nothing was pushed"
    )
}

fn fetch(root: &Path, base: &str, tip: &str) -> Result<String> {
    git::git(root, &["fetch", "--quiet", "origin", "dev"])
        .map_err(|e| format!("ship: could not fetch to re-check the base: {e}"))?;
    if base != "dev" {
        git::git(root, &["fetch", "--quiet", "origin", base]).map_err(|error| {
            if error.contains(&format!("couldn't find remote ref {base}")) {
                restack(base, tip, "the parent branch disappeared during the ship")
            } else {
                format!("ship: could not fetch to re-check the base: {error}")
            }
        })?;
    }
    Ok(git::git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &format!("refs/remotes/origin/{base}^{{commit}}"),
        ],
    )?)
}

/// The commits `latest` has over `current`, oldest first.
fn gained(root: &Path, current: &str, latest: &str) -> Result<Vec<String>> {
    let range = format!("{current}..{latest}");
    Ok(git::git(root, &["rev-list", "--reverse", &range])?
        .lines()
        .map(str::to_owned)
        .collect())
}

/// Replay only the branch's own commits onto the new base. A conflict aborts
/// the rebase and tries a merge of the base instead (the rebase replays every
/// commit, including earlier merge resolutions); only when both conflict is the
/// branch left exactly as the round found it.
fn rebase(
    root: &Path,
    options: &Options,
    latest: &str,
    merge_base: &str,
    round_head: &str,
    when: &str,
) -> Result<()> {
    let Err(error) = git::git(root, &["rebase", "--quiet", "--onto", latest, merge_base]) else {
        return Ok(());
    };
    let files = git::git(root, &["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default();
    let aborted = git::git(root, &["rebase", "--abort"]);
    // A rebase that never started has nothing to abort: the abort fails and
    // the branch is untouched. Only a rebase still in progress, or a HEAD that
    // is not the round's, means the abort did not restore the branch.
    let rebasing = ["rebase-merge", "rebase-apply"].iter().any(|dir| {
        git::git(root, &["rev-parse", "--git-path", dir]).is_ok_and(|path| root.join(path).exists())
    });
    let restored = git::git(root, &["rev-parse", "HEAD"]).is_ok_and(|head| head == round_head);
    if rebasing || !restored {
        return Err(format!(
            "ship: {} {when} and the rebase failed ({error}); the abort did not \
             restore {} ({}): run `git rebase --abort` and check the branch. Nothing was pushed",
            options.base,
            short(round_head),
            aborted.err().unwrap_or_else(|| "abort succeeded".into())
        )
        .into());
    }
    let base = &options.base;
    if !files.is_empty()
        && merge::allowed(root, merge_base, latest, round_head)
        && merge::clean(root, latest, round_head)?
    {
        eprintln!(
            "ship: {base} {when}; the rebase conflicted in {} but a merge of {} is clean, merged instead",
            files.lines().collect::<Vec<_>>().join(", "),
            short(latest)
        );
        return Ok(());
    }
    if files.is_empty() {
        return Err(format!(
            "ship: {base} {when} and the rebase onto it \
             could not run: {error}. The branch is as it was; nothing was pushed"
        )
        .into());
    }
    Err(format!(
        "ship: {base} {when} and conflicts in {}; rebase, resolve, ship again: \
         the review is reused if your patch is unchanged, otherwise only the changed part is \
         reviewed. The branch is as it was ({}); nothing was pushed",
        files.lines().collect::<Vec<_>>().join(", "),
        short(round_head)
    )
    .into())
}

/// Every `static` gate of the preflight cadence, whole-repository: the
/// structure, format, docs and architecture checks and `lint`, which compiles
/// the workspace (clippy), so no separate compile check is added.
fn static_gates(root: &Path, slots: &SlotEnv) -> Result<Vec<GateResult>> {
    let registry = Registry::load(root)?;
    let gates: Vec<&Gate> = registry
        .gates
        .iter()
        .filter(|gate| gate.r#static && gate.cadences.contains(&Cadence::Preflight))
        .collect();
    eprintln!(
        "ship: re-check: {} static gate(s) on the rebased result",
        gates.len()
    );
    let results = run::run_gates(root, &gates, slots);
    for result in &results {
        eprintln!("{}", result.line());
    }
    Ok(results)
}

fn failure(options: &Options, failed: &GateResult, gained: &[String]) -> String {
    let shas: Vec<&str> = gained.iter().map(|sha| short(sha)).collect();
    format!(
        "ship: re-check FAILED: {} {}\n--- {} (last 40 lines) ---\n{}\n\
         This is from the combination with {} new commit(s) on {} ({}). The rebased branch \
         is in place: fix it, commit, and ship again. Nothing was pushed",
        failed.gate,
        match failed.verdict {
            GateVerdict::Unavailable => "could not run",
            _ => "failed",
        },
        failed.gate,
        failed.tail,
        gained.len(),
        options.base,
        shas.join(" ")
    )
}

/// After a rebase of a branch already on the remote, the push overwrites it.
/// Only the branch's own remote is ever lease-forced, and only from the
/// draft-pushed head (otherwise the starting commit), its GitHub dev merges,
/// or an earlier push of the same ordered patches.
pub(super) fn lease(
    root: &Path,
    options: &Options,
    branch: &str,
    head: &str,
    from_head: &str,
) -> Result<Option<String>> {
    let tip = git::git(
        root,
        &["ls-remote", "origin", &format!("refs/heads/{branch}")],
    )?;
    let Some(tip) = tip.split_whitespace().next() else {
        return Ok(None);
    };
    // The updater may have merged dev, or origin may hold our earlier push.
    // Validate with the same rules used immediately before push.
    if tip != from_head
        && git::lease::refresh(root, branch, from_head, from_head, head)
            .is_ok_and(|head| head == tip)
    {
        return Ok(Some(tip.to_owned()));
    }
    if tip == from_head {
        return Ok(Some(from_head.to_owned()));
    }
    let behind = git::git(root, &["merge-base", "--is-ancestor", tip, head]).is_ok();
    if behind || options.force {
        return Ok(None);
    }
    Err(format!(
        "ship: origin/{} is at {}, not at {} where this ship started, and the re-check rebase \
         rewrote the branch: check nobody else pushed there, then ship again with SHIP_FORCE=1. \
         Nothing was pushed",
        branch,
        short(tip),
        short(from_head)
    )
    .into())
}

/// The description's section for the re-check, empty when none ran. The
/// static gates that ran on the rebased head are the evidence's own table
/// (above this section); the full preflight is listed under the commit it ran on.
pub(super) fn section(evidence: &Evidence) -> String {
    let Some(recheck) = &evidence.recheck else {
        return String::new();
    };
    let mut out = format!(
        "\n## Re-check before push\n\n`{}` moved during the ship ({} new commit(s): {}). The \
         branch was rebased from `{}` onto `{}` ({} round(s)). The whole-repository static \
         gates ran on the rebased head `{}` (the table above); the full preflight below passed \
         on `{}` against base `{}`, before the rebase, and was not run again. The passed review \
         was carried over because the own patch and the review policy are unchanged.\n\n\
         ### Full preflight on `{}` (base `{}`)\n\n| Gate | Verdict | Time |\n|---|---|---|\n",
        evidence.base_branch,
        recheck.commits.len(),
        recheck
            .commits
            .iter()
            .map(|sha| format!("`{}`", &sha[..8.min(sha.len())]))
            .collect::<Vec<_>>()
            .join(" "),
        short(&recheck.from_head),
        short(&recheck.base),
        recheck.rounds,
        short(&recheck.head),
        short(&recheck.from_head),
        short(&recheck.from_base),
        short(&recheck.from_head),
        short(&recheck.from_base),
    );
    for gate in &recheck.preflight {
        let verdict = match gate.verdict {
            GateVerdict::Pass => "✅ PASS",
            GateVerdict::Fail => "❌ FAIL",
            GateVerdict::Unavailable => "⚠️ UNAVAILABLE",
        };
        out.push_str(&format!(
            "| {} | {verdict} | {:.0}s |\n",
            gate.gate, gate.seconds
        ));
    }
    out
}
