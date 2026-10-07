//! The review step of `make ship`: a passing adversarial review of the exact
//! commit is required before anything is pushed. On success the review is
//! appended to the pull request description, a `harness/review` commit status
//! is posted and GitHub auto-merge is armed, so GitHub merges once every
//! required check passes. No agent runs a merge command itself.
use super::{Options, evidence::Evidence, gh, git};
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

/// A final fix is the fixes only: at most this many changed lines.
pub(crate) const FINAL_FIX_LINES: u64 = 150;

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
    if let Some(report) = Report::load_passing(root, &evidence.head, &eligible)? {
        eprintln!(
            "ship: reusing the passing {} review of this commit",
            report.tier
        );
        return Ok(report);
    }
    let task = match &options.body_file {
        Some(path) => fs::read_to_string(path)?,
        None => String::new(),
    };
    let history = review::history(root, &evidence.branch)?;
    let plan = match closing::next(&history, &evidence.head) {
        Next::Review(plan) => plan,
        Next::FixFirst(why) | Next::Unavailable(why) => {
            return Err(format!("ship: {why}; nothing was pushed").into());
        }
        Next::Split(why) => return Err(split(&evidence.branch, &why).into()),
        Next::FinalFix(reviewed) => {
            final_fix_scope(root, &reviewed.head, &evidence.head)?;
            let report = closing::final_fix(&reviewed, &evidence.head);
            report.store(root)?;
            eprintln!(
                "ship: final fix: the review budget is spent and converging; this commit ships on tests and gates. Open an issue asking for a post-merge review of {} (docs/review.md)",
                short(&evidence.head)
            );
            return Ok(report);
        }
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
    let next = match closing::next(&history, "") {
        _ if !report.complete() => "the review was incomplete and does not count; ship again (the same commit may be reviewed again)".to_owned(),
        Next::Split(why) => split(&evidence.branch, &why),
        Next::Unavailable(why) => why,
        Next::FinalFix(_) => format!("fix the last closing review's findings, each with a test that fails without the fix, in at most {FINAL_FIX_LINES} changed lines, commit, and ship again: that commit ships as the final fix, with no further model review (docs/review.md)"),
        Next::Review(Plan::Closing { .. }) => "fix and commit every confirmed finding, then ship again: the next review is a closing review of your fixes (docs/review.md)".to_owned(),
        _ => "fix the confirmed findings (never weaken a test or gate), commit, and ship again".to_owned(),
    };
    Err(format!("ship: {verdict}; nothing was pushed. Next: {next}").into())
}

/// A final fix carries only the fixes: few changed lines, and a test with them.
fn final_fix_scope(root: &Path, reviewed: &str, head: &str) -> Result<()> {
    let numstat = git::git(root, &["diff", "--numstat", reviewed, head])?;
    let (mut lines, mut tests) = (0u64, false);
    for line in numstat.lines() {
        let mut parts = line.split('\t');
        let added: u64 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        let removed: u64 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        lines += added + removed;
        tests |= parts
            .next()
            .is_some_and(|path| crate::agents::is_test_path(&path.to_ascii_lowercase()));
    }
    if lines > FINAL_FIX_LINES || !tests {
        return Err(format!(
            "ship: a final fix is the fixes only, each with a test: at most {FINAL_FIX_LINES} changed lines including a test file since {} (found {lines} lines{}); nothing was pushed. Move anything else to a new pull request",
            short(reviewed),
            if tests { "" } else { ", no test" }
        )
        .into());
    }
    Ok(())
}

/// The findings are not converging: the change is too big to fix by
/// iteration, and the agent splits it (no person steps in).
fn split(branch: &str, why: &str) -> String {
    format!(
        "{why}: the reviews of `{branch}` are not converging. Split the change into smaller pull requests on new branches (each gets its own review budget), close this one, and file what is left as issues (docs/review.md)"
    )
}

fn short(sha: &str) -> &str {
    &sha[..12.min(sha.len())]
}

/// The description file plus the review, for the pull request.
pub(crate) fn description(
    root: &Path,
    options: &Options,
    report: &Report,
) -> Result<Option<std::path::PathBuf>> {
    let Some(body) = &options.body_file else {
        return Ok(None);
    };
    let mut text = fs::read_to_string(body)?;
    text.push_str("\n\n## Adversarial review\n\n");
    text.push_str(&report.markdown());
    // A private directory in the git common dir, never a guessable /tmp path.
    let path = super::evidence::directory(root)?
        .with_file_name("bodies")
        .join(format!("{}.md", report.head));
    fs::create_dir_all(path.parent().ok_or("no body directory")?)?;
    fs::write(&path, text)?;
    Ok(Some(path))
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
    let description = format!(
        "{}grade {}/10 · {} tier · {} rounds",
        match (report.closing, report.final_fix) {
            (true, _) => "closing review passed · ",
            (_, true) => "final fix after the review budget · ",
            _ => "",
        },
        report.grade,
        report.tier,
        report.rounds
    );
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
            "pr", "merge", url, "--repo", repository, "--auto", "--squash",
        ]),
    ])
}

/// Post `harness/review` on the commit and arm auto-merge on the pull request.
pub(crate) fn publish(root: &Path, url: &str, report: &Report) -> Result<()> {
    let repository = git::origin_repository(root)?;
    for call in publish_calls(&repository, url, report)? {
        gh(root, &call.iter().map(String::as_str).collect::<Vec<_>>())?;
    }
    Ok(())
}
