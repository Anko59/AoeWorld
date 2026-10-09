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

#[cfg(test)]
mod tests;

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
    let tier = tier(root, options)?;
    let eligible: Vec<&str> = TIERS
        .iter()
        .filter(|t| **t >= tier)
        .map(|t| t.name())
        .collect();
    // The description is part of what was reviewed: reuse needs the same.
    let task = match &options.body_file {
        Some(path) => fs::read_to_string(path)?,
        None => String::new(),
    };
    if let Some(report) = Report::load_passing(root, &evidence.head, &eligible, &task)? {
        eprintln!(
            "ship: reusing the passing {} review of this commit",
            report.tier
        );
        return Ok(report);
    }
    let has_complete = Report::has_complete(root, &evidence.head, &eligible)?;
    // A complete failing review is an explicit verdict for this commit. It
    // must not be replaced by a review of another commit's change.
    if !has_complete
        && let Some(report) =
            Report::reuse_for_change(root, &evidence.head, &evidence.branch, &eligible, &task)?
    {
        eprintln!(
            "ship: review of {} reused (same change, identical file blobs)",
            short(report.reused_from.as_deref().unwrap_or_default())
        );
        return Ok(report);
    }
    let history = review::history(root, &evidence.branch)?;
    let plan = match closing::next(&history, &evidence.head, unix_now()) {
        Next::Review(plan) => plan,
        Next::FixFirst(why) | Next::Unavailable(why) => {
            return Err(format!("ship: {why}; nothing was pushed").into());
        }
        Next::Split(why) => return Err(split(&evidence.branch, &why).into()),
    };
    eprintln!(
        "ship: {}{} review on {}",
        tier.name(),
        if matches!(plan, Plan::Closing { .. }) {
            " closing"
        } else {
            ""
        },
        crate::review::config::runtime_key(options.runtime)
    );
    let report = reviewer(root, tier, options.runtime, &task, &plan)?;
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
        _ if !report.complete() => "the review was incomplete and does not count; ship again (the same commit may be reviewed again)".to_owned(),
        Next::Split(why) => split(&evidence.branch, &why),
        Next::Unavailable(why) => why,
        Next::Review(Plan::Closing { .. }) => "fix and commit every confirmed finding, then ship again: the next review is a closing review of your fixes (docs/review.md)".to_owned(),
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

/// The `gh` calls that publish a passing review: the `harness/review`
/// success status on the reviewed commit, then GitHub auto-merge (squash).
pub(crate) fn publish_calls(
    repository: &str,
    url: &str,
    report: &Report,
) -> Result<Vec<Vec<String>>> {
    if !report.passes() {
        return Err(format!("a {}/10 review publishes nothing", report.grade).into());
    }
    let description = if let Some(source) = &report.reused_from {
        format!(
            "review of {} reused (same change, identical file blobs)",
            short(source)
        )
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
    Ok(vec![
        owned(&[
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
        ]),
        owned(&[
            "pr",
            "merge",
            url,
            "--repo",
            repository,
            "--auto",
            "--squash",
            "--match-head-commit",
            &report.head,
        ]),
    ])
}

/// Post `harness/review` on the commit and arm auto-merge on the pull request.
pub(crate) fn publish(root: &Path, url: &str, report: &Report) -> Result<()> {
    let repository = git::origin_repository(root)?;
    for call in publish_calls(&repository, url, report)? {
        super::github::publish_call(root, &call)?;
    }
    Ok(())
}
