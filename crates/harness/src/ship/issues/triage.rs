//! Record failing nightly jobs in the configured workstream.
use super::{
    Result, Target,
    routing::{self, Config},
};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

const RESULTS_LIMIT: usize = 64 * 1024;
const JOB_CAP: usize = 64;
const OUTPUT_CAP: usize = 64;
/// Failing lines quoted per job, and the characters kept of each.
const FAILURE_LINES: usize = 5;
const LINE_CHARS: usize = 240;

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
    let failures: Vec<_> = results
        .into_iter()
        .filter(|(_, outcome)| *outcome == Outcome::Failure)
        .map(|(job, _)| job)
        .collect();
    if failures.is_empty() {
        return Ok(());
    }
    let config = Config::load(root)?;
    let target = match Target::direct(root) {
        Ok(target) => target,
        Err(error) => {
            routing::warn(config.nightly, &error);
            return Ok(());
        }
    };
    let run = run_link(&target.repo, run_id);
    let jobs = run_jobs(root, &target, run_id);
    let mut body = format!("### Nightly results for {run}\n");
    body.push_str("\nNightly failures:\n");
    for job in failures {
        match jobs.iter().find(|candidate| candidate.name == job) {
            Some(found) => body.push_str(&describe(root, &target, found)),
            None => body.push_str(&format!("- [ ] `{job}` failed\n")),
        }
    }
    routing::comment(root, config.nightly, &body);
    Ok(())
}

#[derive(Deserialize)]
struct RunJob {
    id: u64,
    name: String,
    html_url: String,
    #[serde(default)]
    steps: Vec<RunStep>,
}

#[derive(Deserialize)]
struct RunStep {
    name: String,
    number: u32,
    conclusion: Option<String>,
}

#[derive(Deserialize)]
struct RunJobs {
    jobs: Vec<RunJob>,
}

/// The run's jobs with their steps; none when the run id is missing or GitHub
/// cannot say, so the comment then keeps its plain line.
fn run_jobs(root: &Path, target: &Target, run_id: Option<&str>) -> Vec<RunJob> {
    let Some(id) = run_id.filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
    else {
        return Vec::new();
    };
    let fetched = api(
        root,
        target,
        &format!("actions/runs/{id}/jobs?per_page=100"),
    )
    .and_then(|raw| Ok(serde_json::from_str::<RunJobs>(&raw)?));
    fetched.map(|run| run.jobs).unwrap_or_else(|error| {
        eprintln!("warning: nightly jobs of run {id} unavailable ({error})");
        Vec::new()
    })
}

fn api(root: &Path, target: &Target, path: &str) -> Result<String> {
    let repository = target
        .repo
        .split_once('/')
        .map_or(target.repo.as_str(), |(_, rest)| rest);
    super::gh_call(
        root,
        &target.host,
        &[
            "api",
            "--hostname",
            &target.host,
            &format!("repos/{repository}/{path}"),
        ],
        None,
    )
}

/// One failed job: its first failed step linked to that step's log, then the
/// failing tests and errors its log names.
fn describe(root: &Path, target: &Target, job: &RunJob) -> String {
    let failed = job
        .steps
        .iter()
        .find(|step| step.conclusion.as_deref() == Some("failure"));
    let mut text = match failed {
        Some(step) => format!(
            "- [ ] `{}` failed at step `{}`: {}#step:{}:1\n",
            job.name, step.name, job.html_url, step.number
        ),
        None => format!("- [ ] `{}` failed: {}\n", job.name, job.html_url),
    };
    match api(root, target, &format!("actions/jobs/{}/logs", job.id)) {
        Ok(log) => {
            for line in failure_lines(&log) {
                text.push_str(&format!("  - `{line}`\n"));
            }
        }
        Err(error) => eprintln!("warning: log of job `{}` unavailable ({error})", job.name),
    }
    text
}

/// The lines of a job log that name a failure: nextest `FAIL`, pytest
/// `FAILED`, Playwright `1) [project]` and the harness's `error:`. Each is
/// stripped of its timestamp and backticks, listed once, at most
/// `FAILURE_LINES` of `LINE_CHARS`.
pub(super) fn failure_lines(log: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for raw in log.lines() {
        let line = match raw.split_once(' ') {
            Some((stamp, rest)) if stamp.contains('T') && stamp.ends_with('Z') => rest,
            _ => raw,
        }
        .trim();
        let digits = line.bytes().take_while(u8::is_ascii_digit).count();
        let playwright = digits > 0 && line[digits..].starts_with(") [");
        if !(line.starts_with("FAIL ")
            || line.starts_with("FAIL [")
            || line.starts_with("FAILED ")
            || line.starts_with("error:")
            || playwright)
        {
            continue;
        }
        let line: String = line
            .chars()
            .filter(|c| *c != '`')
            .take(LINE_CHARS)
            .collect();
        if !found.contains(&line) {
            found.push(line);
        }
        if found.len() == FAILURE_LINES {
            break;
        }
    }
    found
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

pub(super) fn run_link(repo: &str, run_id: Option<&str>) -> String {
    match run_id.filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())) {
        Some(id) => format!("https://{repo}/actions/runs/{id}"),
        None => "a run without a GITHUB_RUN_ID".into(),
    }
}

#[cfg(test)]
#[path = "tests/triage.rs"]
mod tests;
