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
    git, review_gate,
    run::{self, GateResult, GateVerdict},
};
use crate::{
    gates::registry::{Cadence, Gate, Registry},
    review::{Report, base},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[cfg(test)]
mod tests;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// How often the base may move again during the re-check before the ship stops.
pub(super) const MAX_ROUNDS: u32 = 3;

/// What the re-check did, kept in the evidence and the pull request description.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Recheck {
    /// The base commit the branch was rebased onto.
    pub(crate) base: String,
    /// The commit the ship started from (before the rebase).
    pub(crate) from_head: String,
    pub(crate) rounds: u32,
    /// The commits the base gained meanwhile, oldest first.
    pub(crate) commits: Vec<String>,
    /// The static gates of the last round, on the rebased result.
    pub(crate) gates: Vec<GateResult>,
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
/// move under the ship.
pub(super) fn before_push(
    root: &Path,
    evidence: &Evidence,
    options: &Options,
) -> Result<Option<Rebased>> {
    if base::is_local(&options.base) {
        return Ok(None);
    }
    let from_head = evidence.head.clone();
    let mut current = evidence.base.clone();
    let mut merge_base = evidence.merge_base.clone();
    let (mut rounds, mut commits, mut results) = (0, Vec::new(), Vec::new());
    loop {
        let latest = fetch(root, &options.base)?;
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
            gained.len(),
        )?;
        eprintln!(
            "ship: {} moved by {} commit(s) during the ship; rebased onto {}, re-checking",
            options.base,
            gained.len(),
            short(&latest)
        );
        results = static_gates(root)?;
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
    let lease = lease(root, options, &subject, &from_head)?;
    let anchor = base::scope_at(&options.base, &current);
    let evidence = Evidence {
        head: subject.head,
        tree: subject.tree,
        base: current.clone(),
        changed: git::changed(root, &merge_base)?,
        merge_base,
        recheck: Some(Recheck {
            base: current,
            from_head,
            rounds,
            commits,
            gates: results,
        }),
        finished: super::now(),
        ..evidence.clone()
    };
    let path = evidence::write(root, &evidence)?;
    eprintln!("ship: evidence {}", path.display());
    let review = if options.no_review {
        None
    } else {
        Some(carry_review(root, &evidence, options)?)
    };
    Ok(Some(Rebased {
        evidence,
        review,
        lease,
        _anchor: anchor,
    }))
}

fn fetch(root: &Path, base: &str) -> Result<String> {
    let mut args = vec!["fetch", "--quiet", "origin", "dev"];
    if base != "dev" {
        args.push(base);
    }
    git::git(root, &args)
        .map_err(|e| format!("ship: could not fetch to re-check the base: {e}"))?;
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

/// Replay only the branch's own commits onto the new base; a conflict aborts
/// the rebase and leaves the branch exactly as the round found it.
fn rebase(
    root: &Path,
    options: &Options,
    latest: &str,
    merge_base: &str,
    round_head: &str,
    gained: usize,
) -> Result<()> {
    let Err(error) = git::git(root, &["rebase", "--quiet", "--onto", latest, merge_base]) else {
        return Ok(());
    };
    let files = git::git(root, &["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default();
    let aborted = git::git(root, &["rebase", "--abort"]);
    let restored = git::git(root, &["rev-parse", "HEAD"]).is_ok_and(|head| head == round_head);
    if aborted.is_err() || !restored {
        return Err(format!(
            "ship: {} moved during the ship and the rebase failed ({error}); the abort did not \
             restore {}: run `git rebase --abort` and check the branch. Nothing was pushed",
            options.base,
            short(round_head)
        )
        .into());
    }
    let base = &options.base;
    if files.is_empty() {
        return Err(format!(
            "ship: {base} moved during the ship ({gained} new commit(s)) and the rebase onto it \
             failed: {error}. The branch is as it was; nothing was pushed"
        )
        .into());
    }
    Err(format!(
        "ship: {base} moved during the ship and conflicts in {}; rebase, resolve, ship again: \
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
fn static_gates(root: &Path) -> Result<Vec<GateResult>> {
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
    let results = run::run_gates(root, &gates);
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
/// commit this ship started from; any other remote tip needs `SHIP_FORCE`.
fn lease(
    root: &Path,
    options: &Options,
    subject: &git::Subject,
    from_head: &str,
) -> Result<Option<String>> {
    let tip = git::git(
        root,
        &[
            "ls-remote",
            "origin",
            &format!("refs/heads/{}", subject.branch),
        ],
    )?;
    let Some(tip) = tip.split_whitespace().next() else {
        return Ok(None);
    };
    if tip == from_head {
        return Ok(Some(from_head.to_owned()));
    }
    let behind = git::git(root, &["merge-base", "--is-ancestor", tip, &subject.head]).is_ok();
    if behind || options.force {
        return Ok(None);
    }
    Err(format!(
        "ship: origin/{} is at {}, not at {} where this ship started, and the re-check rebase \
         rewrote the branch: check nobody else pushed there, then ship again with SHIP_FORCE=1. \
         Nothing was pushed",
        subject.branch,
        short(tip),
        short(from_head)
    )
    .into())
}

/// The review of the rebased commit comes from the stored one by patch
/// identity; if the rebase changed the patch it is not run here.
fn carry_review(root: &Path, evidence: &Evidence, options: &Options) -> Result<Report> {
    review_gate::require(root, evidence, options, &|_, _, _, _, _| {
        Err(
            "the re-check rebase changed your patch, so the passed review cannot be carried \
             over; ship again to review the changed part"
                .into(),
        )
    })
}

/// The description's section for the re-check, empty when none ran.
pub(super) fn section(evidence: &Evidence) -> String {
    let Some(recheck) = &evidence.recheck else {
        return String::new();
    };
    let mut out = format!(
        "\n## Re-check before push\n\n`{}` moved during the ship ({} new commit(s): {}). The \
         branch was rebased from `{}` onto `{}` ({} round(s)) and the whole-repository static \
         gates ran on the result; the passed review was carried over by patch identity.\n\n\
         | Gate | Verdict | Time |\n|---|---|---|\n",
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
        recheck.rounds
    );
    for gate in &recheck.gates {
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
