//! Immutable selected mutation inputs and bounded candidate evidence; no judge authority.
use crate::{
    gates::scopes::{self, Kind, Snapshot},
    perf::Verdict,
};
use std::{error::Error, path::Path};
mod execution;
mod integrity;
mod io;
mod outcomes;
mod publication;
mod storage;
use outcomes::Outcomes;
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TOOL_VERSION: &str = "27.1.0";
const FILTER: &str = "compare|classify|selection";
const EXCLUDE: &str = " in client$";
#[cfg(test)]
const OUTPUT: &str = "reports/mutation/campaign";
const FILES: [&str; 3] = [
    "crates/harness/src/perf.rs",
    "crates/harness/src/gates.rs",
    "crates/harness/src/gates/registry/mod.rs",
];

#[derive(Debug, Default, clap::Args)]
pub(crate) struct Options {
    #[arg(long,value_parser=full_revision,conflicts_with="intentional_index")]
    revision: Option<String>,
    #[arg(long)]
    intentional_index: bool,
}
fn full_revision(value: &str) -> std::result::Result<String, String> {
    if !matches!(value.len(), 40 | 64)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("mutation revision must be full lowercase SHA-1 or SHA-256".into());
    }
    Ok(value.into())
}
fn prepare(source: &Path, options: &Options) -> Result<Snapshot> {
    if options.intentional_index {
        if options.revision.is_some() {
            return Err("mutation revision and intentional index conflict".into());
        }
        return Snapshot::prepare(source, Kind::Index);
    }
    let default = options.revision.is_none();
    let revision = match &options.revision {
        Some(value) => full_revision(value)?,
        None => scopes::resolved_head(source)?,
    };
    let snapshot = Snapshot::prepare_independent(source, Kind::Commit(revision.clone()))?;
    if default && snapshot.identity.source_head != revision {
        return Err("source HEAD changed while admitting default mutation commit".into());
    }
    Ok(snapshot)
}
fn assess(outcomes: &Outcomes, command_succeeded: bool) -> (Verdict, Option<String>) {
    if outcomes.cargo_mutants_version != TOOL_VERSION
        || outcomes.end_time.as_deref().is_none_or(str::is_empty)
        || !outcomes.counters_valid()
        || outcomes.evaluated().is_none_or(|count| count < 30)
    {
        return (
            Verdict::Inconclusive,
            Some("mutation tool identity, completion or evaluated sample count is invalid".into()),
        );
    }
    if outcomes.missed > 0 || outcomes.timeout > 0 {
        return (
            Verdict::Regression,
            Some(format!(
                "{} missed and {} timed-out critical mutations",
                outcomes.missed, outcomes.timeout
            )),
        );
    }
    if outcomes.success != 0 {
        return (
            Verdict::Inconclusive,
            Some("non-Test successful mutant records do not qualify evaluated evidence".into()),
        );
    }
    if !command_succeeded {
        return (
            Verdict::Inconclusive,
            Some("mutation command failed despite complete outcomes".into()),
        );
    }
    (Verdict::Pass, None)
}
pub fn run_integrity() -> Result<()> {
    integrity::run()
}
pub fn run(options: Options) -> Result<()> {
    let source = std::env::current_dir()?;
    // This precedes options/Git/admission/canary/command work, not merely reads.
    publication::invalidate(&source)?;
    let preparation = (|| -> Result<(Snapshot, execution::Execution)> {
        let snapshot = prepare(&source, &options)?;
        integrity::run()?;
        let execution = execution::execute(&source, &snapshot)?;
        Ok((snapshot, execution))
    })();
    let (snapshot, mut execution) = match preparation {
        Ok(value) => value,
        Err(error) => {
            let _ = publication::preparation_failed(&source);
            return Err(error);
        }
    };
    let publication = publication::observe_execution(&source, &snapshot, &execution);
    if let Ok(report) = &publication {
        println!(
            "mutation campaign: {:?}, {:?} caught, {:?} missed",
            report.verdict, report.caught, report.missed
        );
    }
    // Original actual process failure takes precedence over secondary failures,
    // while a writable report records source/artifact/publication failures too.
    match execution.command.take() {
        Some(Err(error)) => return Err(error.into()),
        None => {
            return Err(execution
                .endpoint_error
                .take()
                .unwrap_or_else(|| "mutation command was not started".into()));
        }
        Some(Ok(())) => {}
    }
    if let Some(error) = execution.endpoint_error.take() {
        return Err(error);
    }
    let report = publication?;
    if report.verdict != Verdict::Pass {
        return Err(report
            .failure
            .unwrap_or_else(|| "mutation campaign inconclusive or regressed".into())
            .into());
    }
    // Publication already performs the final source checks while its ORIGINAL
    // artifact FDs are still held; do not add an unpaired after-drop check here.
    Ok(())
}
#[cfg(test)]
use crate::process;
#[cfg(test)]
use publication::{ArtifactFailure, CommandObservation, Report, invalidate};
#[cfg(test)]
fn write_report(root: &Path, command: &Result<()>) -> Result<Report> {
    publication::invalidate(root)?;
    // Explicit synthetic fixture adapter only. Production always receives the
    // retained Execution's fresh directory and immutable Snapshot context.
    publication::observe(
        root,
        command,
        &io::absolute(root)?.join(OUTPUT).join("mutants.out"),
    )
}
#[cfg(test)]
mod tests;
