//! `make perf-dev-reference`: hand the instruction gate the latest passing
//! `dev` measurement. The gate itself runs without network or token, so this
//! step fetches the same dev artifact the performance comment uses and leaves
//! `reports/perf/dev-reference.json`. Any failure becomes an `unavailable`
//! record with its reason (the gate then states it and keeps the baseline
//! checks); only a failure to write the file fails the step.
use super::{Result, dev_reports, reports::Report};
use crate::perf_micro::dev::{DevFile, DevReference, PATH};
use crate::ship::git;
use std::{fs, path::Path};

/// The dev reference from the newest passing dev run, or why there is none.
pub(crate) fn file_from(devs: Result<Vec<(u64, Report)>>) -> DevFile {
    let unavailable = |reason: String| DevFile::Unavailable {
        unavailable: reason,
    };
    match devs {
        Err(error) => unavailable(format!("dev runs could not be read: {error}")),
        Ok(devs) => match devs.into_iter().next() {
            None => unavailable("no passing dev run with a performance artifact".to_owned()),
            Some((run, report)) => DevFile::Measured(DevReference {
                run,
                revision: report.revision,
                workloads: report.instruction_workloads,
                instructions: report
                    .instructions
                    .into_iter()
                    .filter_map(|(name, count)| Some((name, count?)))
                    .collect(),
            }),
        },
    }
}

pub(crate) fn run() -> Result<()> {
    let root = Path::new(".");
    let scratch = tempfile::tempdir().map_err(|error| format!("temporary directory: {error}"))?;
    let devs = git::origin_repository(root)
        .map_err(|error| error.to_string())
        .and_then(|repo| dev_reports(root, &repo, scratch.path(), 1));
    let file = file_from(devs);
    match &file {
        DevFile::Unavailable { unavailable } => {
            println!("perf-dev-reference: unavailable ({unavailable})");
        }
        DevFile::Measured(reference) => println!(
            "perf-dev-reference: dev run {} ({})",
            reference.run, reference.revision
        ),
    }
    let bytes = serde_json::to_vec_pretty(&file).map_err(|error| error.to_string())?;
    fs::create_dir_all("reports/perf").map_err(|error| error.to_string())?;
    fs::write(PATH, bytes).map_err(|error| format!("{PATH}: {error}"))
}
