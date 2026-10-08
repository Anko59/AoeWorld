//! `make nightly-triage`: the nightly workflow's last job reads its `needs`
//! context from `NIGHTLY_RESULTS` and keeps one issue per failing job
//! (docs/issues.md). A failing job opens its issue or comments on it; a job
//! that passes again closes it. Only issues this command opened are touched:
//! authored by `github-actions[bot]`, labelled `nightly-failure`, titled
//! `Nightly failure: <job>` and carrying that title's fingerprint marker.
use super::{Issue, Result, Target, fingerprint};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

const RESULTS_LIMIT: usize = 64 * 1024;
const JOB_CAP: usize = 64;
const OUTPUT_CAP: usize = 64;
const NIGHTLY_LABEL: &str = "nightly-failure";
const BOT: &str = "github-actions[bot]";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Need {
    result: Outcome,
    #[serde(default)]
    outputs: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum Outcome {
    Success,
    Failure,
    Cancelled,
    Skipped,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Action {
    Open { title: String, body: String },
    Comment { number: u64, body: String },
    Close { number: u64, comment: String },
}

pub(in crate::ship) fn execute(root: &Path) -> Result<()> {
    if !crate::agents::main_session() {
        return Err("make nightly-triage is available only to the main session and CI".into());
    }
    let raw = std::env::var("NIGHTLY_RESULTS")
        .map_err(|_| "set NIGHTLY_RESULTS to the nightly workflow's `needs` context JSON")?;
    triage(root, &raw, std::env::var("GITHUB_RUN_ID").ok().as_deref())
}

pub(super) fn triage(root: &Path, raw: &str, run_id: Option<&str>) -> Result<()> {
    let results = parse(raw)?;
    let target = Target::load(root)?;
    let run = run_link(&target.repo, run_id);
    let mut problems = Vec::new();
    for action in plan(&results, &target.issues, &run) {
        let done = match &action {
            Action::Open { title, body } => {
                target.create(root, title, &[NIGHTLY_LABEL.to_owned()], body)
            }
            Action::Comment { number, body } => target.gh(
                root,
                &["issue", "comment", &number.to_string(), "--body", body],
                None,
            ),
            Action::Close { number, comment } => target.gh(
                root,
                &["issue", "close", &number.to_string(), "--comment", comment],
                None,
            ),
        };
        match done {
            Ok(_) => println!("nightly-triage: {action:?}"),
            Err(error) => problems.push(format!("{action:?}: {error}")),
        }
    }
    if problems.is_empty() {
        return Ok(());
    }
    Err(format!("nightly-triage: {}", problems.join("; ")).into())
}

/// The `needs` context: at most 64 KiB, 64 jobs with GitHub job ids, each
/// with a known result and string outputs, and nothing else.
pub(super) fn parse(raw: &str) -> Result<BTreeMap<String, Outcome>> {
    if raw.len() > RESULTS_LIMIT {
        return Err(format!("NIGHTLY_RESULTS exceeds {RESULTS_LIMIT} bytes").into());
    }
    let needs: BTreeMap<String, Need> = serde_json::from_str(raw)
        .map_err(|error| format!("NIGHTLY_RESULTS is not a `needs` context: {error}"))?;
    if needs.is_empty() || needs.len() > JOB_CAP {
        return Err(format!("NIGHTLY_RESULTS must name 1 to {JOB_CAP} jobs").into());
    }
    let mut results = BTreeMap::new();
    for (job, need) in needs {
        if !job_id(&job) {
            return Err(format!("NIGHTLY_RESULTS names an invalid job id `{job}`").into());
        }
        if need.outputs.len() > OUTPUT_CAP {
            return Err(format!("job `{job}` has more than {OUTPUT_CAP} outputs").into());
        }
        results.insert(job, need.result);
    }
    Ok(results)
}

/// A GitHub Actions job id: a letter or `_`, then letters, digits, `-` or `_`.
fn job_id(job: &str) -> bool {
    let mut bytes = job.bytes();
    job.len() <= 64
        && bytes
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub(super) fn title(job: &str) -> String {
    format!("Nightly failure: {job}")
}

/// Only an issue this command opened: the bot's, labelled, titled for `job`
/// and with the last fingerprint marker matching that title.
pub(super) fn owned(issue: &Issue, job: &str) -> bool {
    let title = title(job);
    issue.author == BOT
        && issue.title == title
        && issue.has_label(NIGHTLY_LABEL)
        && issue.fingerprint_marker.as_deref() == Some(fingerprint(&title).as_str())
}

/// The run on origin's repository (`host/owner/name`), never GITHUB_REPOSITORY.
fn run_link(repo: &str, run_id: Option<&str>) -> String {
    match run_id.filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())) {
        Some(id) => format!("https://{repo}/actions/runs/{id}"),
        None => "a run without a GITHUB_RUN_ID".into(),
    }
}

pub(super) fn plan(
    results: &BTreeMap<String, Outcome>,
    issues: &[Issue],
    run: &str,
) -> Vec<Action> {
    let mut actions = Vec::new();
    for (job, outcome) in results {
        let mut mine = issues.iter().filter(|issue| owned(issue, job));
        match outcome {
            Outcome::Failure => actions.push(match mine.next() {
                Some(issue) => Action::Comment {
                    number: issue.number,
                    body: format!("Still failing in {run}."),
                },
                None => Action::Open {
                    title: title(job),
                    body: format!(
                        "The nightly job `{job}` failed in {run}.\n\n`make nightly-triage` comments here while it keeps failing and closes this issue when it passes again (docs/issues.md).\n"
                    ),
                },
            }),
            Outcome::Success => actions.extend(mine.map(|issue| Action::Close {
                number: issue.number,
                comment: format!("Passing again in {run}."),
            })),
            // A cancelled or skipped job proved nothing either way.
            Outcome::Cancelled | Outcome::Skipped => {}
        }
    }
    actions
}

#[cfg(test)]
#[path = "tests/triage.rs"]
mod tests;
