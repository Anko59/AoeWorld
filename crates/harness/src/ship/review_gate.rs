//! The review step of `make ship`: a passing adversarial review of the exact
//! commit is required before anything is pushed. On success the review is
//! appended to the pull request description, a `harness/review` commit status
//! is posted and GitHub auto-merge is armed, so GitHub merges once every
//! required check passes. No agent runs a merge command itself.
use super::{Options, evidence::Evidence, gh, git};
use crate::review::{self, Report, Tier};
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
    eprintln!(
        "ship: {} review on {}",
        tier.name(),
        crate::review::config::runtime_key(options.runtime)
    );
    let report = reviewer(root, tier, options.runtime, &task)?;
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
    let verdict = format!(
        "review {}/10 (needs {}{})",
        report.grade,
        report.merge_grade,
        if report.failures.is_empty() {
            ""
        } else {
            ", and the review was incomplete"
        }
    );
    let next = "fix the confirmed findings (never weaken a test or gate), commit, and ship again";
    Err(format!("ship: {verdict}; nothing was pushed. Next: {next}").into())
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
        "grade {}/10 · {} tier · {} rounds",
        report.grade, report.tier, report.rounds
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
