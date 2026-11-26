//! Reliability, from the GitHub Actions API: per job, how many of the runs
//! of `ci.yml` on pushes to `dev` failed each week, and which runs failed on
//! their first attempt and passed on a rerun. Completed runs never change
//! except by a rerun (which raises `run_attempt`), so they are cached under
//! `.cache/ci-report/` and a second command fetches only runs it has not seen.
use super::calendar;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

type Result<T> = std::result::Result<T, String>;
/// A `gh` call: its arguments in, its standard output out.
pub(super) type Gh<'a> = &'a (dyn Fn(&[&str]) -> Result<String> + Sync);

const WORKFLOW: &str = "ci.yml";
const PAGE_SIZE: usize = 100;
const MAX_PAGES: usize = 20;
const WORKERS: usize = 8;
const CACHE: &str = "actions.json";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ActionJob {
    pub(crate) name: String,
    pub(crate) conclusion: Option<String>,
    /// The first step that failed.
    pub(crate) step: Option<String>,
}

/// A completed run: its final attempt's jobs and, for a rerun, the first attempt's.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ActionRun {
    pub(crate) id: u64,
    pub(crate) attempt: u32,
    pub(crate) conclusion: Option<String>,
    pub(crate) created_at: String,
    pub(crate) jobs: Vec<ActionJob>,
    pub(crate) first_attempt: Option<Vec<ActionJob>>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RunHead {
    pub(super) id: u64,
    #[serde(rename = "run_attempt")]
    pub(super) attempt: u32,
    pub(super) conclusion: Option<String>,
    pub(super) created_at: String,
}

pub(super) fn parse_runs(text: &str) -> Result<Vec<RunHead>> {
    #[derive(Deserialize)]
    struct Page {
        workflow_runs: Vec<RunHead>,
    }
    let page: Page = serde_json::from_str(text).map_err(|error| format!("runs: {error}"))?;
    Ok(page.workflow_runs)
}

pub(super) fn parse_jobs(text: &str) -> Result<Vec<ActionJob>> {
    #[derive(Deserialize)]
    struct Step {
        name: String,
        conclusion: Option<String>,
    }
    #[derive(Deserialize)]
    struct Job {
        name: String,
        conclusion: Option<String>,
        #[serde(default)]
        steps: Vec<Step>,
    }
    #[derive(Deserialize)]
    struct Page {
        jobs: Vec<Job>,
    }
    let page: Page = serde_json::from_str(text).map_err(|error| format!("jobs: {error}"))?;
    Ok(page
        .jobs
        .into_iter()
        .map(|job| ActionJob {
            step: job
                .steps
                .into_iter()
                .find(|step| step.conclusion.as_deref() == Some("failure"))
                .map(|step| step.name),
            name: job.name,
            conclusion: job.conclusion,
        })
        .collect())
}

fn fetch_run(gh: Gh, repo: &str, head: &RunHead) -> Result<ActionRun> {
    let jobs = |suffix: &str| -> Result<Vec<ActionJob>> {
        let path = format!(
            "repos/{repo}/actions/runs/{}{suffix}/jobs?per_page={PAGE_SIZE}",
            head.id
        );
        parse_jobs(&gh(&["api", &path])?).map_err(|error| format!("run {}: {error}", head.id))
    };
    Ok(ActionRun {
        id: head.id,
        attempt: head.attempt,
        conclusion: head.conclusion.clone(),
        created_at: head.created_at.clone(),
        jobs: jobs("")?,
        first_attempt: if head.attempt > 1 {
            Some(jobs("/attempts/1")?)
        } else {
            None
        },
    })
}

fn heads(gh: Gh, repo: &str, since_day: &str) -> Result<Vec<RunHead>> {
    let mut heads = Vec::new();
    for page in 1..=MAX_PAGES {
        let path = format!(
            "repos/{repo}/actions/workflows/{WORKFLOW}/runs?branch=dev&event=push&status=completed&per_page={PAGE_SIZE}&page={page}&created=%3E%3D{since_day}"
        );
        let found = parse_runs(&gh(&["api", &path])?)?;
        let last = found.len() < PAGE_SIZE;
        heads.extend(found);
        if last {
            break;
        }
    }
    Ok(heads)
}

/// The completed `dev` push runs created on or after `since_day`, oldest
/// first, with the warnings of what could not be read.
pub(super) fn fetch(
    cache_dir: &Path,
    repo: &str,
    since_day: &str,
    gh: Gh,
) -> (Vec<ActionRun>, Vec<String>) {
    let file = cache_dir.join(CACHE);
    let mut cached: BTreeMap<u64, ActionRun> = fs::read_to_string(&file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let mut warnings = Vec::new();
    let heads = match heads(gh, repo, since_day) {
        Ok(heads) => heads,
        Err(error) => {
            warnings.push(format!("Actions runs unavailable: {error}"));
            let runs = cached
                .into_values()
                .filter(|r| calendar::day(&r.created_at) >= since_day);
            return (runs.collect(), warnings);
        }
    };
    let pending: Vec<&RunHead> = heads
        .iter()
        .filter(|head| {
            !cached
                .get(&head.id)
                .is_some_and(|run| run.attempt == head.attempt && run.conclusion == head.conclusion)
        })
        .collect();
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<Result<ActionRun>>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..WORKERS.min(pending.len()) {
            scope.spawn(|| {
                while let Some(head) = pending.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let result = fetch_run(gh, repo, head);
                    if let Ok(mut finished) = done.lock() {
                        finished.push(result);
                    }
                }
            });
        }
    });
    for result in done
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
    {
        match result {
            Ok(run) => {
                cached.insert(run.id, run);
            }
            Err(error) => warnings.push(error),
        }
    }
    if !pending.is_empty()
        && let Err(error) = fs::create_dir_all(cache_dir).and_then(|()| {
            let text = serde_json::to_string(&cached).map_err(std::io::Error::other)?;
            fs::write(&file, text)
        })
    {
        warnings.push(format!("{}: {error}", file.display()));
    }
    let ids: Vec<u64> = heads.iter().map(|head| head.id).collect();
    let mut runs: Vec<ActionRun> = cached
        .into_values()
        .filter(|run| ids.contains(&run.id))
        .collect();
    runs.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    (runs, warnings)
}

/// One job's record over the runs: the end-to-end shards count as one job.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct JobReliability {
    pub(crate) name: String,
    pub(crate) runs: u32,
    pub(crate) failed: u32,
    /// Week (its Monday) to (runs, failed runs).
    pub(crate) weeks: BTreeMap<String, (u32, u32)>,
    /// Runs where this job failed on the first attempt and the run passed on a rerun.
    pub(crate) recovered: u32,
    /// The step that most often failed first.
    pub(crate) step: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Reliability {
    pub(crate) weeks: Vec<String>,
    pub(crate) jobs: Vec<JobReliability>,
    pub(crate) runs: u32,
    /// Runs whose final attempt had a failed job.
    pub(crate) runs_failed: u32,
    /// Runs that failed on the first attempt and passed on a rerun.
    pub(crate) runs_recovered: u32,
}

/// Name to (failed, first failed step) of the jobs that ran to success or failure.
fn outcomes(jobs: &[ActionJob]) -> BTreeMap<String, (bool, Option<String>)> {
    let mut out: BTreeMap<String, (bool, Option<String>)> = BTreeMap::new();
    for job in jobs {
        let failed = match job.conclusion.as_deref() {
            Some("success") => false,
            Some("failure") => true,
            _ => continue,
        };
        let name = if job.name.starts_with("e2e (") {
            "e2e"
        } else {
            job.name.as_str()
        };
        let entry = out.entry(name.to_owned()).or_default();
        if failed && !entry.0 {
            *entry = (true, job.step.clone());
        }
    }
    out
}

pub(super) fn reliability(runs: &[ActionRun]) -> Reliability {
    let mut jobs: BTreeMap<String, JobReliability> = BTreeMap::new();
    let mut steps: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
    let mut weeks = Vec::new();
    let (mut runs_failed, mut runs_recovered) = (0, 0);
    for run in runs {
        let Some(week) = calendar::week_start(&run.created_at) else {
            continue;
        };
        if !weeks.contains(&week) {
            weeks.push(week.clone());
        }
        let last = outcomes(&run.jobs);
        runs_failed += u32::from(last.values().any(|(failed, _)| *failed));
        let first = run.first_attempt.as_deref().map(outcomes);
        let recovered = run.conclusion.as_deref() == Some("success");
        runs_recovered += u32::from(
            recovered
                && first
                    .as_ref()
                    .is_some_and(|f| f.values().any(|(failed, _)| *failed)),
        );
        for (name, (failed, step)) in &last {
            let job = jobs.entry(name.clone()).or_insert_with(|| JobReliability {
                name: name.clone(),
                runs: 0,
                failed: 0,
                weeks: BTreeMap::new(),
                recovered: 0,
                step: None,
            });
            job.runs += 1;
            job.failed += u32::from(*failed);
            let week = job.weeks.entry(week.clone()).or_default();
            *week = (week.0 + 1, week.1 + u32::from(*failed));
            if let Some(step) = step.as_ref().filter(|_| *failed) {
                *steps
                    .entry(name.clone())
                    .or_default()
                    .entry(step.clone())
                    .or_default() += 1;
            }
        }
        for (name, (failed, step)) in first.iter().flatten() {
            if !failed {
                continue;
            }
            if recovered && let Some(job) = jobs.get_mut(name) {
                job.recovered += 1;
            }
            if let Some(step) = step {
                *steps
                    .entry(name.clone())
                    .or_default()
                    .entry(step.clone())
                    .or_default() += 1;
            }
        }
    }
    for (name, job) in &mut jobs {
        job.step = steps.get(name).and_then(|counts| {
            counts
                .iter()
                .max_by_key(|(step, count)| (**count, std::cmp::Reverse((*step).clone())))
                .map(|(step, _)| step.clone())
        });
    }
    weeks.sort();
    Reliability {
        weeks,
        jobs: jobs.into_values().collect(),
        runs: u32::try_from(runs.len()).unwrap_or(u32::MAX),
        runs_failed,
        runs_recovered,
    }
}
