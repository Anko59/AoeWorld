//! The pull request performance comment (item 12 of #388): what CI's
//! `target-performance` job measured, posted as ONE comment under a hidden
//! marker and updated in place by later runs. Informational only: no gate,
//! baseline or threshold is read from it or changed by it.
//!
//! `make perf-comment` runs this after that job on pull requests. It reads
//! the run's `performance-report` artifact and the latest one from a
//! successful `dev` run, renders one compact table, and creates or rewrites
//! the marked pull request comment. `PERF_COMMENT_PRINT=1` prints the body
//! instead of posting (local checks).
mod render;
mod reports;
#[cfg(test)]
mod tests;

use crate::ship::git;
use render::Noise;
use reports::Report;
use serde::Deserialize;
use std::{fs, path::Path};

pub(crate) struct Job {
    pub(crate) name: String,
    pub(crate) conclusion: Option<String>,
    pub(crate) seconds: Option<u64>,
}

type Result<T> = std::result::Result<T, String>;

/// `ship::gh` with this module's `String` error type.
fn gh_text(root: &Path, args: &[&str]) -> Result<String> {
    crate::ship::gh(root, args).map_err(|error| error.to_string())
}

const WORKFLOW: &str = "ci.yml";
const DEV_BRANCH: &str = "dev";
/// The dev runs tried before giving up: cancelled runs crowd the list.
const DEV_RUN_CANDIDATES: usize = 30;
const ARTIFACT: &str = "performance-report";

fn env_number(name: &str) -> Result<Option<u64>> {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => value
            .parse::<u64>()
            .map(Some)
            .map_err(|_| format!("{name}={value}: not a number")),
        _ => Ok(None),
    }
}

/// The run to describe: the workflow sets `AOE_RUN_ID`, a plain CI
/// environment also has `GITHUB_RUN_ID`.
fn run_id() -> Result<u64> {
    env_number("AOE_RUN_ID")?
        .or(env_number("GITHUB_RUN_ID")?)
        .ok_or_else(|| "AOE_RUN_ID is unset: no CI run to describe".to_owned())
}

fn download(root: &Path, repo: &str, run: u64, dir: &Path) -> Result<()> {
    gh_text(
        root,
        &[
            "run",
            "download",
            &run.to_string(),
            "--repo",
            repo,
            "--name",
            ARTIFACT,
            "--dir",
            &dir.display().to_string(),
        ],
    )
    .map(|_| ())
    .map_err(|error| format!("run {run}: no {ARTIFACT} artifact ({error})"))
}

/// The measured report of one run, downloaded into `scratch/run-{id}`.
fn report_of(root: &Path, repo: &str, run: u64, scratch: &Path) -> Option<Report> {
    let dir = scratch.join(format!("run-{run}"));
    download(root, repo, run, &dir).ok()?;
    reports::read(&dir).ok()
}

/// The successful `dev` runs of the CI workflow that uploaded the artifact,
/// newest first, at most `limit` of them. Missing or expired artifacts fall
/// through to earlier runs; cancelled runs crowd the list.
fn dev_reports(
    root: &Path,
    repo: &str,
    scratch: &Path,
    limit: usize,
) -> Result<Vec<(u64, Report)>> {
    let ids = gh_text(
        root,
        &[
            "run",
            "list",
            "--repo",
            repo,
            "--branch",
            DEV_BRANCH,
            "--workflow",
            WORKFLOW,
            "--limit",
            &DEV_RUN_CANDIDATES.to_string(),
            "--json",
            "databaseId,conclusion",
            "--jq",
            ".[] | select(.conclusion == \"success\") | .databaseId",
        ],
    )?;
    let mut reports = Vec::new();
    for id in ids.lines() {
        if reports.len() >= limit {
            break;
        }
        let Ok(run) = id.trim().parse::<u64>() else {
            continue;
        };
        if let Some(report) = report_of(root, repo, run, scratch) {
            reports.push((run, report));
        }
    }
    Ok(reports)
}

/// Whole seconds between two RFC 3339 timestamps (`…Z`).
pub(crate) fn epoch_seconds(timestamp: &str) -> Option<i64> {
    let (date, time) = timestamp.strip_suffix('Z')?.split_once('T')?;
    let mut date = date.split('-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: i64 = date.next()?.parse().ok()?;
    let day: i64 = date.next()?.parse().ok()?;
    let mut time = time.split(':');
    let hour: i64 = time.next()?.parse().ok()?;
    let minute: i64 = time.next()?.parse().ok()?;
    let second: i64 = time.next().unwrap_or("0").split('.').next()?.parse().ok()?;
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

pub(crate) fn format_duration(seconds: u64) -> String {
    let (minutes, rest) = (seconds / 60, seconds % 60);
    if minutes >= 60 {
        format!("{}h{:02}m", minutes / 60, minutes % 60)
    } else if minutes > 0 {
        format!("{minutes}m{rest:02}s")
    } else {
        format!("{rest}s")
    }
}

#[derive(Deserialize)]
struct JobsJson {
    jobs: Vec<JobJson>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobJson {
    name: String,
    conclusion: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
}

fn wall_seconds(job: &JobJson) -> Option<u64> {
    let start = epoch_seconds(job.started_at.as_deref()?)?;
    let end = epoch_seconds(job.completed_at.as_deref()?)?;
    u64::try_from(end.saturating_sub(start)).ok()
}

fn jobs(root: &Path, repo: &str, run: u64) -> Result<Vec<Job>> {
    let text = gh_text(
        root,
        &[
            "run",
            "view",
            &run.to_string(),
            "--repo",
            repo,
            "--json",
            "jobs",
        ],
    )?;
    let parsed: JobsJson =
        serde_json::from_str(&text).map_err(|error| format!("run {run}: jobs: {error}"))?;
    Ok(parsed
        .jobs
        .into_iter()
        .map(|job| Job {
            seconds: wall_seconds(&job),
            name: job.name,
            conclusion: job.conclusion,
        })
        .collect())
}

/// The first comment id in `gh`'s jq output (one id per line).
pub(crate) fn comment_id(output: &str) -> Option<u64> {
    output
        .lines()
        .next()
        .and_then(|line| line.trim().parse::<u64>().ok())
}

/// The comment to rewrite, found by its hidden marker in the pull request's
/// comments.
fn existing_comment(root: &Path, repo: &str, pr: u64) -> Result<Option<u64>> {
    let jq = format!(
        ".[] | select(.body | contains(\"{}\")) | .id",
        render::MARKER
    );
    let output = gh_text(
        root,
        &[
            "api",
            "--paginate",
            &format!("repos/{repo}/issues/{pr}/comments?per_page=100"),
            "--jq",
            &jq,
        ],
    )?;
    Ok(comment_id(&output))
}

/// The `gh` arguments that update the existing comment in place, or create
/// it when no marked comment exists yet.
pub(crate) fn comment_args(
    existing: Option<u64>,
    repo: &str,
    pr: u64,
    body_file: &Path,
) -> Vec<String> {
    match existing {
        Some(id) => vec![
            "api".to_owned(),
            "--method".to_owned(),
            "PATCH".to_owned(),
            format!("repos/{repo}/issues/comments/{id}"),
            "-F".to_owned(),
            format!("body=@{}", body_file.display()),
        ],
        None => vec![
            "pr".to_owned(),
            "comment".to_owned(),
            pr.to_string(),
            "--repo".to_owned(),
            repo.to_owned(),
            "--body-file".to_owned(),
            body_file.display().to_string(),
        ],
    }
}

fn upsert(root: &Path, repo: &str, pr: u64, body: &str) -> Result<()> {
    let dir = tempfile::tempdir().map_err(|error| format!("temporary directory: {error}"))?;
    let file = dir.path().join("comment.md");
    fs::write(&file, body).map_err(|error| format!("{}: {error}", file.display()))?;
    let args = comment_args(existing_comment(root, repo, pr)?, repo, pr, &file);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    gh_text(root, &argv).map(|_| ())
}

pub(crate) fn run() -> Result<()> {
    let root = Path::new(".");
    let print = std::env::var("PERF_COMMENT_PRINT").ok().as_deref() == Some("1");
    let pr = env_number("AOE_PR_NUMBER")?;
    let run = run_id()?;
    let repo = git::origin_repository(root)?;
    let scratch = tempfile::tempdir().map_err(|error| format!("temporary directory: {error}"))?;
    let pr_report = report_of(root, &repo, run, scratch.path());
    let noise = Noise::load()?;
    let devs = dev_reports(root, &repo, scratch.path(), noise.dev_runs())?;
    let job_list = jobs(root, &repo, run)?;
    let wasm = reports::wasm_rule_at(Path::new("baselines/perf/wasm.json"));
    let body = render::comment(render::Input {
        repository: &repo,
        run,
        devs: &devs,
        pr: pr_report.as_ref(),
        jobs: &job_list,
        wasm,
        noise: &noise,
    });
    if print {
        println!("{body}");
        return Ok(());
    }
    let pr =
        pr.ok_or("AOE_PR_NUMBER is unset: posting the comment needs the pull request number")?;
    upsert(root, &repo, pr, &body)?;
    println!("perf-comment: posted the performance comment on pull request #{pr}");
    Ok(())
}
