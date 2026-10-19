//! The review step of `make ship`: a passing adversarial review of the exact
//! commit is required before anything is pushed. On success the review is
//! appended to the pull request description, a `harness/review` commit status
//! is posted and GitHub auto-merge is armed, so GitHub merges once every
//! required check passes. No agent runs a merge command itself.
use super::{Options, evidence::Evidence, git};
use crate::review::{
    self, Plan, Report, Tier,
    closing::{self, Next},
};
use std::{fs, path::Path};

mod why;

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(in crate::ship) use tests::{graded, publish_calls};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Runs a review: `review::review`, or a scripted one in tests.
pub(crate) type Reviewer<'a> = dyn Fn(
        &Path,
        Tier,
        crate::agents::Runtime,
        &str,
        &Plan,
    ) -> std::result::Result<Report, Box<dyn std::error::Error>>
    + 'a;

const TIERS: [Tier; 5] = [Tier::Low, Tier::Medium, Tier::High, Tier::Xhigh, Tier::Max];

/// The tier to run: the requested one, which may not be below the floor.
fn tier(root: &Path, options: &Options) -> Result<Tier> {
    let floor = review::floor(root)?;
    match options.tier {
        Some(tier) if tier < floor => Err(format!(
            "this change needs at least a {} review (SHIP_TIER={})",
            floor.name(),
            tier.name()
        )
        .into()),
        Some(tier) => Ok(tier),
        None => Ok(floor),
    }
}

/// A passing review of `evidence.head`: reused if stored, otherwise run now.
pub(crate) fn require(
    root: &Path,
    evidence: &Evidence,
    options: &Options,
    reviewer: &Reviewer<'_>,
) -> Result<Report> {
    let task = match &options.body_file {
        Some(path) => fs::read_to_string(path)?,
        None => String::new(),
    };
    require_task(root, evidence, options, &task, reviewer)
}

/// Print the `review:` line for the review about to run and remember it for the
/// report that review stores: the one announcement of `make ship` and
/// `make review`.
pub(crate) fn announce(plan: &Plan, why: &str) -> String {
    let line = why::line(plan, why);
    eprintln!("{line}");
    review::announce(&line);
    line
}

/// Same review step with already-frozen task text (the review-only entry).
pub(crate) fn require_task(
    root: &Path,
    evidence: &Evidence,
    options: &Options,
    task: &str,
    reviewer: &Reviewer<'_>,
) -> Result<Report> {
    let tier = tier(root, options)?;
    let eligible: Vec<&str> = TIERS
        .iter()
        .filter(|t| **t >= tier)
        .map(|t| t.name())
        .collect();
    if let Some(report) = Report::load_passing(root, &evidence.head, &eligible, task)? {
        eprintln!(
            "review: reusing the passing {} review of this commit",
            report.tier
        );
        return Ok(report);
    }
    let why = match Report::reuse_decision(root, &evidence.head, &evidence.branch, &eligible, task)?
    {
        review::Reuse::Reused(report) => {
            eprintln!("{}", report.decision.as_deref().unwrap_or_default());
            return Ok(*report);
        }
        review::Reuse::Declined(reason) => reason,
    };
    let history = review::history(root, &evidence.branch)?;
    let plan = match review::increment::next(root, &history, &evidence.head, unix_now()) {
        Next::Review(plan) => plan,
        Next::FixFirst(why) | Next::Unavailable(why) => {
            return Err(format!("ship: {why}; nothing was pushed").into());
        }
        Next::Split(why) => return Err(split(&evidence.branch, &why).into()),
    };
    let line = announce(&plan, &why);
    eprintln!(
        "ship: {}{} review on {}",
        tier.name(),
        match &plan {
            Plan::Closing { .. } => " closing",
            Plan::Increment(i) if i.closing => " closing (increment)",
            Plan::Increment(_) => " full (increment)",
            Plan::Full => "",
        },
        crate::review::config::runtime_key(options.runtime)
    );
    let detail = serde_json::json!({"tier":tier.name(), "closing":plan.closing_style()});
    crate::timeline::mark(root, "review", "start", detail.clone());
    let reviewed = reviewer(root, tier, options.runtime, task, &plan);
    let mut ended = detail;
    match &reviewed {
        Ok(report) => {
            ended["grade"] = serde_json::json!(report.grade);
            ended["rounds"] = serde_json::json!(report.rounds);
            ended["ok"] = serde_json::json!(report.passes());
        }
        Err(_) => ended["ok"] = serde_json::json!(false),
    }
    crate::timeline::mark(root, "review", "end", ended);
    drop(review::take_announced());
    let mut report = reviewed?;
    report.decision.get_or_insert(line);
    eprintln!("{}", report.markdown());
    // A review takes minutes: only a review of the very commit the gates
    // passed, still checked out, unlocks the push.
    let now = git::git(root, &["rev-parse", "HEAD"])?;
    if report.head != evidence.head || now != evidence.head {
        return Err(format!(
            "ship: HEAD moved while the review ran (gated {}, reviewed {}, now {}); nothing was pushed. Ship again",
            short(&evidence.head),
            short(&report.head),
            short(&now)
        )
        .into());
    }
    if report.base != evidence.base || report.base_branch != evidence.base_branch {
        return Err(
            "ship: the review answered for another base than the gates; nothing was pushed".into(),
        );
    }
    if report.passes() {
        return Ok(report);
    }
    let verdict = if report.closing {
        format!(
            "closing review failed ({} blocking finding(s) left{})",
            report.blocking().len(),
            if report.complete() {
                ""
            } else {
                ", incomplete"
            }
        )
    } else {
        format!(
            "review {}/10 (needs {}{})",
            report.grade,
            report.merge_grade,
            if report.complete() {
                ""
            } else {
                ", and the review was incomplete"
            }
        )
    };
    let history = review::history(root, &evidence.branch)?;
    let next = match closing::next(&history, "", unix_now()) {
        _ if !report.complete() => report.incomplete_message(),
        Next::Split(why) => split(&evidence.branch, &why),
        Next::Unavailable(why) => why,
        Next::Review(Plan::Closing { .. }) => "fix and commit every confirmed finding, then ship again: the next review is a closing review of your fixes since the last reviewed commit and every carried blocking finding (docs/review.md)".to_owned(),
        _ => "fix the confirmed findings (never weaken a test or gate), commit, and ship again".to_owned(),
    };
    Err(format!("ship: {verdict}; nothing was pushed. Next: {next}").into())
}

/// The review budget is spent: the change is too big to fix by iteration,
/// and the agent splits it (no person steps in).
fn split(branch: &str, why: &str) -> String {
    format!(
        "{why}: the review budget of `{branch}` is spent. Split the change into smaller pull requests on new branches (each gets its own review budget), close this one, and file what is left as issues (docs/review.md)"
    )
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn short(sha: &str) -> &str {
    &sha[..12.min(sha.len())]
}

/// The publication calls for a pull request into `base`: auto-merge is armed only
/// into dev, never into a stacked parent branch, which GitHub would merge
/// into without the parent's review.
fn publish_calls_onto(
    repository: &str,
    url: &str,
    report: &Report,
    base: &str,
) -> Result<Vec<Vec<String>>> {
    if !report.passes() {
        return Err(format!("a {}/10 review publishes nothing", report.grade).into());
    }
    let description = if let Some(source) = &report.reused_from {
        format!(
            "review of {} reused (same patch){}",
            short(source),
            report
                .note
                .as_ref()
                .map(|note| format!("; {note}"))
                .unwrap_or_default()
        )
    } else if let Some(increment) = &report.increment {
        format!("passed: {}", increment.sentence())
    } else {
        format!(
            "{}grade {}/10 · {} tier · {} rounds",
            if report.closing {
                "closing review passed · "
            } else {
                ""
            },
            report.grade,
            report.tier,
            report.rounds
        )
    };
    let owned = |args: &[&str]| args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
    let mut calls = vec![owned(&[
        "api",
        "--method",
        "POST",
        &format!("repos/{repository}/statuses/{}", report.head),
        "-f",
        "state=success",
        "-f",
        "context=harness/review",
        "-f",
        &format!("description={description}"),
    ])];
    if base != crate::review::base::DEV {
        return Ok(calls);
    }
    calls.push(owned(&[
        "pr",
        "merge",
        url,
        "--repo",
        repository,
        "--auto",
        "--squash",
        "--match-head-commit",
        &report.head,
    ]));
    Ok(calls)
}

/// Post `harness/review` on the commit and, into dev, arm auto-merge on the
/// pull request.
pub(crate) fn publish(root: &Path, url: &str, report: &Report, base: &str) -> Result<()> {
    let repository = git::origin_repository(root)?;
    // Both ship and review-pr publish here: every approval records the marker
    // before its status. Marker failure warns; only the later carry would fail.
    if let Err(error) = super::postpush::marker::record(root, url, report) {
        eprintln!(
            "review: the review marker was not written ({error}); a rebase will not carry the review"
        );
    }
    for call in publish_calls_onto(&repository, url, report, base)? {
        super::github::publish_call(root, &call)?;
    }
    Ok(())
}
