//! `make ci-report`: one offline HTML page of the CI's numbers over time, and
//! the data it draws as JSON (`reports/ci/index.html`, `reports/ci/data.json`).
//!
//! The numbers come from the performance comment CI leaves on every pull
//! request (`perf_comment`): its `dev` column and the `dev` commit it names
//! give the instruction counts and bundle sizes of `dev` over time, its
//! job table gives how long each job took. Commit dates come from the
//! `origin/dev` log; baselines, redefinitions and accepted increases from the
//! history of `baselines/perf/*.json`; reliability from the GitHub Actions API.
//! Raw comments and job lists are cached under `.cache/ci-report/`, so a
//! second run fetches only what is new.
mod actions;
mod calendar;
mod chart;
mod charts;
mod comments;
mod fetch;
mod format;
mod history;
mod page;
mod series;
#[cfg(test)]
mod tests;
mod timing;

use actions::{ActionRun, Reliability};
use comments::Observation;
use history::{Event, Version};
use serde::Serialize;
use series::{Commit, DevPoint, Move, Standing};
use std::{error::Error, fs, path::Path, time::Instant};
use timing::{Day, RunTime};

const MICRO: &str = "baselines/perf/micro.json";
const WASM: &str = "baselines/perf/wasm.json";
const CACHE: &str = ".cache/ci-report";
const OUT: &str = "reports/ci";
/// Reliability looks back this many days of runs on `dev`.
const RELIABILITY_DAYS: i64 = 56;

/// The committed limit of the client bundle from one commit on.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct BundleLimit {
    pub(crate) position: usize,
    pub(crate) baseline: u64,
    /// The largest size the gate accepts.
    pub(crate) limit: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct Inputs {
    pub(crate) comment_versions: usize,
    pub(crate) dev_commits: usize,
    pub(crate) measured_commits: usize,
    pub(crate) runs_with_jobs: usize,
    pub(crate) actions_runs: usize,
}

/// Everything the page draws and `data.json` keeps.
#[derive(Debug, Serialize)]
pub(crate) struct Data {
    pub(crate) generated_at: String,
    pub(crate) repository: String,
    pub(crate) headline: String,
    pub(crate) inputs: Inputs,
    pub(crate) dev: Vec<DevPoint>,
    pub(crate) events: Vec<Event>,
    pub(crate) standing: Vec<Standing>,
    pub(crate) moves: Vec<Move>,
    pub(crate) bundle: Vec<BundleLimit>,
    pub(crate) ci_runs: Vec<RunTime>,
    pub(crate) ci_days: Vec<Day>,
    pub(crate) reliability: Option<Reliability>,
    pub(crate) warnings: Vec<String>,
}

/// What was read, before any of it is computed on.
pub(super) struct Sources {
    pub(super) repository: String,
    pub(super) now: String,
    pub(super) commits: Vec<Commit>,
    pub(super) comment_versions: usize,
    pub(super) observations: Vec<Observation>,
    pub(super) micro: Vec<Version>,
    pub(super) bundle: Vec<BundleLimit>,
    pub(super) actions: Option<Vec<ActionRun>>,
    pub(super) warnings: Vec<String>,
}

pub(super) fn assemble(sources: Sources) -> Data {
    let dev = series::dev_points(&sources.commits, &sources.observations);
    let events = history::events(&sources.micro);
    let wasm = sources.bundle.last().map(|limit| limit.baseline);
    let standing = sources
        .micro
        .last()
        .map(|latest| series::standing(&dev, latest, wasm))
        .unwrap_or_default();
    let ci_runs = timing::runs(&sources.observations);
    let mut data = Data {
        generated_at: sources.now,
        repository: sources.repository,
        headline: String::new(),
        inputs: Inputs {
            comment_versions: sources.comment_versions,
            dev_commits: sources.commits.len(),
            measured_commits: dev.len(),
            runs_with_jobs: ci_runs.len(),
            actions_runs: sources.actions.as_ref().map_or(0, Vec::len),
        },
        moves: series::moves(&dev, &events),
        ci_days: timing::daily(&ci_runs),
        reliability: sources.actions.as_deref().map(actions::reliability),
        bundle: sources.bundle,
        dev,
        events,
        standing,
        ci_runs,
        warnings: sources.warnings,
    };
    data.headline = page::headline(&data);
    data
}

/// The versions of `micro.json` with the commit that wrote each.
fn micro_versions(
    commits: &[Commit],
    history: Vec<(usize, String)>,
) -> Result<Vec<Version>, String> {
    history
        .into_iter()
        .map(|(position, text)| {
            let mut version = history::parse_micro(&text)?;
            let commit = &commits[position];
            let (title, pr) = series::split_subject(&commit.subject);
            version.position = position;
            version.at.clone_from(&commit.at);
            (version.title, version.pr) = (title, pr);
            Ok(version)
        })
        .collect()
}

fn bundle_limits(history: Vec<(usize, String)>) -> Result<Vec<BundleLimit>, String> {
    history
        .into_iter()
        .map(|(position, text)| {
            let baseline = history::parse_wasm(&text)?;
            Ok(BundleLimit {
                position,
                baseline,
                limit: crate::perf::regression_floor(baseline) - 1,
            })
        })
        .collect()
}

fn days_ago(now: &str, days: i64) -> String {
    let seconds = crate::perf_comment::epoch_seconds(now).unwrap_or(0) - days * 86_400;
    calendar::day(&calendar::iso(seconds)).to_owned()
}

pub(crate) fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let started = Instant::now();
    let repository = crate::ship::git::origin_repository(root)?;
    let gh = |args: &[&str]| crate::ship::gh(root, args).map_err(|error| error.to_string());
    let git = |args: &[&str]| crate::ship::git::git(root, args).map_err(|error| error.to_string());
    let mut warnings = Vec::new();
    if let Err(error) = git(&["fetch", "--quiet", "origin", "dev"]) {
        warnings.push(format!("origin/dev was not refreshed: {error}"));
    }
    let now = calendar::iso(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64,
    );
    let cache = root.join(CACHE);
    let commits = fetch::dev_commits(&git)?;
    let micro = micro_versions(&commits, fetch::file_history(&git, &commits, MICRO)?)?;
    let bundle = bundle_limits(fetch::file_history(&git, &commits, WASM)?)?;
    let versions = fetch::comments(&cache, &repository, &gh)?;
    let observations: Vec<Observation> = versions
        .iter()
        .filter_map(|v| comments::parse_comment(v.pr, &v.updated, &v.body))
        .collect();
    let (action_runs, action_warnings) =
        actions::fetch(&cache, &repository, &days_ago(&now, RELIABILITY_DAYS), &gh);
    warnings.extend(action_warnings);
    let data = assemble(Sources {
        repository,
        now,
        commits,
        comment_versions: versions.len(),
        observations,
        micro,
        bundle,
        actions: Some(action_runs).filter(|runs| !runs.is_empty()),
        warnings,
    });
    let out = root.join(OUT);
    fs::create_dir_all(&out)?;
    fs::write(out.join("index.html"), page::render(&data))?;
    fs::write(
        out.join("data.json"),
        serde_json::to_string_pretty(&data)? + "\n",
    )?;
    for warning in &data.warnings {
        eprintln!("ci-report: warning: {warning}");
    }
    println!(
        "ci-report: {} dev commits measured, {} CI runs, {} Actions runs in {:.1}s\nci-report: {}",
        data.inputs.measured_commits,
        data.inputs.runs_with_jobs,
        data.inputs.actions_runs,
        started.elapsed().as_secs_f64(),
        out.join("index.html").display()
    );
    Ok(())
}
