//! How long CI took: one row per run from the job tables of the comments,
//! and the median of each job, of a run's longest job and of a run's
//! machine time over each UTC day.
use super::calendar;
use super::comments::Observation;
use serde::Serialize;
use std::collections::BTreeMap;

/// One CI run of a pull request, durations in seconds.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct RunTime {
    pub(crate) run: Option<u64>,
    pub(crate) pr: u64,
    pub(crate) at: String,
    /// Per job that passed; the end-to-end shards collapse into the slowest.
    pub(crate) jobs: BTreeMap<String, u64>,
    pub(crate) longest: u64,
    /// Every job's time added up, failed ones included.
    pub(crate) total: u64,
    pub(crate) failed: Vec<String>,
}

fn collapsed(name: &str) -> &str {
    if name.starts_with("e2e (") {
        "e2e"
    } else {
        name
    }
}

/// The runs listed in the comments, oldest first. A comment is rewritten by
/// each later run of its pull request, so versions of a comment are separate
/// runs, and one run named twice counts once (its later version).
pub(super) fn runs(observations: &[Observation]) -> Vec<RunTime> {
    let mut sorted: Vec<&Observation> = observations
        .iter()
        .filter(|observation| !observation.jobs.is_empty())
        .collect();
    sorted.sort_by(|a, b| a.at.cmp(&b.at));
    let mut seen: BTreeMap<(u64, String), RunTime> = BTreeMap::new();
    for observation in sorted {
        let mut jobs: BTreeMap<String, u64> = BTreeMap::new();
        let mut failed = Vec::new();
        for job in &observation.jobs {
            let name = collapsed(&job.name).to_owned();
            if job.failed {
                if !failed.contains(&name) {
                    failed.push(name);
                }
            } else {
                let slowest = jobs.entry(name).or_default();
                *slowest = (*slowest).max(job.seconds);
            }
        }
        let key = match observation.run {
            Some(run) => (run, String::new()),
            None => (0, format!("{}@{}", observation.pr, observation.at)),
        };
        seen.insert(
            key,
            RunTime {
                run: observation.run,
                pr: observation.pr,
                at: observation.at.clone(),
                jobs,
                longest: observation
                    .jobs
                    .iter()
                    .map(|j| j.seconds)
                    .max()
                    .unwrap_or(0),
                total: observation.jobs.iter().map(|j| j.seconds).sum(),
                failed,
            },
        );
    }
    let mut runs: Vec<RunTime> = seen.into_values().collect();
    runs.sort_by(|a, b| a.at.cmp(&b.at));
    runs
}

/// The median; the mean of the middle pair for an even count, 0 for none.
pub(super) fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    match values.len() {
        0 => 0.0,
        n if n % 2 == 1 => values[n / 2],
        n => (values[n / 2 - 1] + values[n / 2]) / 2.0,
    }
}

/// One UTC day of runs, in minutes.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Day {
    pub(crate) day: String,
    pub(crate) runs: usize,
    /// Median per job, over the runs of the day where the job passed.
    pub(crate) jobs: BTreeMap<String, f64>,
    pub(crate) longest: f64,
    pub(crate) total: f64,
}

fn minutes(mut seconds: Vec<f64>) -> f64 {
    (median(&mut seconds) / 6.0).round() / 10.0
}

pub(super) fn daily(runs: &[RunTime]) -> Vec<Day> {
    let mut days: BTreeMap<&str, Vec<&RunTime>> = BTreeMap::new();
    for run in runs {
        days.entry(calendar::day(&run.at)).or_default().push(run);
    }
    days.into_iter()
        .map(|(day, runs)| {
            let mut names: Vec<&String> = runs.iter().flat_map(|run| run.jobs.keys()).collect();
            names.sort();
            names.dedup();
            let jobs = names
                .into_iter()
                .map(|name| {
                    let seconds = runs.iter().filter_map(|run| run.jobs.get(name));
                    (name.clone(), minutes(seconds.map(|&s| s as f64).collect()))
                })
                .collect();
            Day {
                day: day.to_owned(),
                runs: runs.len(),
                jobs,
                longest: minutes(runs.iter().map(|run| run.longest as f64).collect()),
                total: minutes(runs.iter().map(|run| run.total as f64).collect()),
            }
        })
        .collect()
}
