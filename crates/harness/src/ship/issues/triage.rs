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
    let mut body = format!("### Nightly results for {run}\n");
    body.push_str("\nNightly failures:\n");
    for job in failures {
        body.push_str(&format!("- [ ] `{job}` failed\n"));
    }
    routing::comment(root, config.nightly, &body);
    Ok(())
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
