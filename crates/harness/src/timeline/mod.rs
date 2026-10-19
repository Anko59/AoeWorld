//! Local delivery observations, independent of ship approval/evidence policy.
mod github;
mod history;
mod journal;
mod report;
#[cfg(test)]
mod tests;

pub(crate) use journal::{mark, stage};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Deserialize, Serialize)]
struct Entry {
    t: f64,
    branch: String,
    head: String,
    stage: String,
    event: String,
    detail: Value,
}

#[derive(Clone, Debug)]
struct Sample {
    branch: String,
    head: String,
    stage: String,
    started: f64,
    finished: f64,
    duration: f64,
    detail: Value,
    source: &'static str,
}

impl Sample {
    fn seconds(&self) -> f64 {
        self.duration
    }
}

/// File order matters: a second start replaces an interrupted attempt, never
/// pairs its eventual end with an older process or an earlier head's attempt.
fn pair(entries: Vec<Entry>) -> (Vec<Sample>, usize) {
    let mut open = BTreeMap::new();
    let mut samples = Vec::new();
    let mut unpaired = 0;
    for entry in entries {
        if !entry.t.is_finite() || entry.t < 0. {
            unpaired += 1;
            continue;
        }
        let key = (
            entry.branch.clone(),
            entry.stage.clone(),
            entry.detail["run"].to_string(),
            entry.detail["gate"].to_string(),
        );
        match entry.event.as_str() {
            "start" => unpaired += usize::from(open.insert(key, entry).is_some()),
            "end" => match open.remove(&key) {
                Some(start) if entry.t >= start.t => {
                    let mut detail = start.detail;
                    if let (Some(a), Some(b)) = (detail.as_object_mut(), entry.detail.as_object()) {
                        a.extend(b.clone());
                    }
                    samples.push(Sample {
                        branch: entry.branch,
                        head: entry.head,
                        stage: entry.stage,
                        started: start.t,
                        finished: entry.t,
                        duration: entry.t - start.t,
                        detail,
                        source: "journal",
                    });
                }
                Some(_) => unpaired += 2,
                None => unpaired += 1,
            },
            _ => unpaired += 1,
        }
    }
    unpaired += open.len();
    samples.sort_by(|a, b| a.started.total_cmp(&b.started));
    (samples, unpaired)
}

pub(crate) fn run(root: &Path) -> Result<()> {
    let days: u32 = std::env::var("TIMELINE_DAYS")
        .ok()
        .filter(|s| !s.is_empty())
        .map_or(Ok(7), |s| s.parse())?;
    if days == 0 {
        return Err("TIMELINE_DAYS must be positive".into());
    }
    let until = journal::now();
    let since = (until - f64::from(days) * 86_400.).max(0.);
    let common = crate::ship::git::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let data = history::load(&Path::new(&common).join("aoe-ship"))?;
    let (prs, warnings) = github::read(root, since);
    print!("{}", report::render(&data, &prs, since, until));
    for warning in warnings {
        println!("WARNING: {warning}");
    }
    Ok(())
}
