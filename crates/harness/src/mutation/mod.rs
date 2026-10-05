//! Pinned nightly mutation claims, with bounded structural artifact observation.
use crate::{perf::Verdict, process};
use serde::Serialize;
use std::{error::Error, path::Path, process::Command, time::Duration};
mod io;
mod outcomes;
use outcomes::Outcomes;
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TOOL_VERSION: &str = "27.1.0";
const FILTER: &str = "compare|classify|selection";
const EXCLUDE: &str = " in client$";
const OUTPUT: &str = "reports/mutation/campaign";
const FILES: [&str; 3] = [
    "crates/harness/src/perf.rs",
    "crates/harness/src/gates.rs",
    "crates/harness/src/gates/registry/mod.rs",
];

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ArtifactFailure {
    InputUnavailable,
    SchemaInvalid,
    ChangedDuringObservation,
    GitObservationUnavailable,
}
#[derive(Debug, Serialize)]
struct CommandObservation {
    status: &'static str,
    exit_code: Option<i32>,
    root_cause: &'static str,
}
impl CommandObservation {
    fn observe(result: &Result<()>) -> Self {
        let (status, exit_code) = match result {
            Ok(()) => ("SUCCESS", None),
            Err(error) => match error.downcast_ref::<process::ProcessError>() {
                Some(process::ProcessError::Exit { code, .. }) => ("FAILED", *code),
                Some(process::ProcessError::Deadline { .. }) => ("DEADLINE", None),
                Some(process::ProcessError::Cancelled { .. }) => ("CANCELLED", None),
                Some(process::ProcessError::Start { .. }) => ("START_UNAVAILABLE", None),
                Some(process::ProcessError::Monitor { .. }) => ("MONITOR_UNAVAILABLE", None),
                None => ("FAILED_UNCLASSIFIED", None),
            },
        };
        Self {
            status,
            exit_code,
            root_cause: "ROOT_CAUSE_NOT_ASSESSED",
        }
    }
}
#[derive(Serialize)]
struct Report {
    version: u16,
    assessment: &'static str,
    authoritative: bool,
    source_identity: &'static str,
    served_build_binding: &'static str,
    artifact_freshness: &'static str,
    execution_binding: &'static str,
    root_cause: &'static str,
    revision: Option<String>,
    dirty: Option<bool>,
    tool: &'static str,
    tool_version: &'static str,
    files: [&'static str; 3],
    filter: &'static str,
    excluded_mutants: &'static str,
    mutant_set_hash: Option<String>,
    artifacts: Option<io::Measurements>,
    total_mutants: Option<u64>,
    evaluated_mutants: Option<u64>,
    caught: Option<u64>,
    missed: Option<u64>,
    timeout: Option<u64>,
    unviable: Option<u64>,
    verdict: Verdict,
    failure: Option<String>,
    command_observation: CommandObservation,
    // Legacy local diagnostic, not a safe/public transport or root-cause field.
    command_failure: Option<String>,
    artifact_failure: Option<ArtifactFailure>,
}
fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err(format!("git {args:?} failed").into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
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
fn invalidate(root: &Path) -> Result<()> {
    io::write_known(root, "nightly.json", br#"{"version":2,"verdict":"INCONCLUSIVE","assessment":"PENDING_NON_AUTHORITATIVE","authoritative":false,"source_identity":"UNAVAILABLE","served_build_binding":"UNAVAILABLE","root_cause":"ROOT_CAUSE_NOT_ASSESSED"}"#)?;
    io::write_known(
        root,
        "nightly.md",
        b"# Critical mutation campaign\n\nPENDING / INCONCLUSIVE / NON-AUTHORITATIVE.\n",
    )
}
fn write_report(root: &Path, command_result: &Result<()>) -> Result<Report> {
    // Before ANY fallible artifact/Git work, invalidate a previous local PASS.
    invalidate(root)?;
    let mut report = Report {
        version: 2,
        assessment: "MUTATION_EVIDENCE_UNAVAILABLE_NON_AUTHORITATIVE",
        authoritative: false,
        source_identity: "UNAVAILABLE",
        served_build_binding: "UNAVAILABLE",
        artifact_freshness: "UNAVAILABLE",
        execution_binding: "UNAVAILABLE",
        root_cause: "ROOT_CAUSE_NOT_ASSESSED",
        revision: None,
        dirty: None,
        tool: "cargo-mutants",
        tool_version: TOOL_VERSION,
        files: FILES,
        filter: FILTER,
        excluded_mutants: EXCLUDE,
        mutant_set_hash: None,
        artifacts: None,
        total_mutants: None,
        evaluated_mutants: None,
        caught: None,
        missed: None,
        timeout: None,
        unviable: None,
        verdict: Verdict::Inconclusive,
        failure: Some("mutation artifact evidence unavailable".into()),
        command_observation: CommandObservation::observe(command_result),
        command_failure: command_result.as_ref().err().map(ToString::to_string),
        artifact_failure: None,
    };
    let mut held = match io::Pair::at(root) {
        Ok(pair) => Some(pair),
        Err(_) => {
            report.artifact_failure = Some(ArtifactFailure::InputUnavailable);
            None
        }
    };
    if let Some(pair) = &mut held {
        match outcomes::parse(&pair.outcomes_bytes, &pair.inventory_bytes) {
            Ok(outcomes) => {
                report.total_mutants = Some(outcomes.total_mutants);
                report.evaluated_mutants = outcomes.evaluated();
                report.caught = Some(outcomes.caught);
                report.missed = Some(outcomes.missed);
                report.timeout = Some(outcomes.timeout);
                report.unviable = Some(outcomes.unviable);
                (report.verdict, report.failure) = assess(&outcomes, command_result.is_ok());
                report.assessment = "STRICT_MUTATION_EVIDENCE_OBSERVED_NON_AUTHORITATIVE";
            }
            Err(_) => {
                report.artifact_failure = Some(ArtifactFailure::SchemaInvalid);
            }
        }
    }
    // These AFTER-run observations retain their old CWD/Git-env weakness. They
    // are not an immutable source identity; failed observations cannot yield PASS.
    match (git(&["rev-parse", "HEAD"]), git(&["status", "--porcelain"])) {
        (Ok(revision), Ok(status)) => {
            report.revision = Some(revision);
            report.dirty = Some(!status.is_empty());
        }
        _ => {
            report.artifact_failure = Some(ArtifactFailure::GitObservationUnavailable);
        }
    }
    if let Some(pair) = &mut held
        && pair.recheck().is_err()
    {
        report.artifact_failure = Some(ArtifactFailure::ChangedDuringObservation);
    }
    if report.artifact_failure.is_some() {
        report.verdict = Verdict::Inconclusive;
        report.failure = Some("mutation artifact or context observation unavailable".into());
        report.assessment = "MUTATION_EVIDENCE_UNAVAILABLE_NON_AUTHORITATIVE";
    }
    // Write human summary BEFORE the accepted machine record. A failed write
    // leaves the machine publication pending, never an accepted previous PASS.
    io::write_known(root, "nightly.md", format!(
        "# Critical mutation campaign\n\nPost-run claimed revision: `{:?}`; dirty: `{:?}`; local verdict: `{:?}`.\n\nSource identity UNAVAILABLE; authoritative false; root cause NOT ASSESSED.\n\nCaught: {:?}; missed: {:?}; timed out: {:?}; unviable: {:?}; total: {:?}; evaluated: {:?}.\n",
        report.revision, report.dirty, report.verdict, report.caught, report.missed,
        report.timeout, report.unviable, report.total_mutants, report.evaluated_mutants).as_bytes())?;
    if let Some(pair) = &mut held {
        if pair.recheck().is_err() {
            report.verdict = Verdict::Inconclusive;
            report.failure = Some("mutation artifact changed before publication".into());
            report.artifact_failure = Some(ArtifactFailure::ChangedDuringObservation);
            report.assessment = "MUTATION_EVIDENCE_UNAVAILABLE_NON_AUTHORITATIVE";
            io::write_known(root, "nightly.md", b"# Critical mutation campaign\n\nINCONCLUSIVE: retained artifact changed; NON-AUTHORITATIVE.\n")?;
        } else if report.artifact_failure.is_none() {
            report.mutant_set_hash = Some(pair.measurements.inventory.raw_blake3.clone());
            report.artifacts = Some(io::Measurements {
                outcomes: io::Measurement {
                    bytes: pair.measurements.outcomes.bytes,
                    raw_blake3: pair.measurements.outcomes.raw_blake3.clone(),
                },
                inventory: io::Measurement {
                    bytes: pair.measurements.inventory.bytes,
                    raw_blake3: pair.measurements.inventory.raw_blake3.clone(),
                },
            });
        }
    }
    let encoded = serde_json::to_vec_pretty(&report)?;
    if let Err(error) = io::write_known(root, "nightly.json", &encoded) {
        let _ = invalidate(root);
        return Err(error);
    }
    Ok(report)
}
fn scanner_args() -> Vec<&'static str> {
    let mut args = vec![
        "mutants",
        "--in-place",
        "--timeout",
        "120",
        "--output",
        OUTPUT,
        "--re",
        FILTER,
        "--exclude-re",
        EXCLUDE,
    ];
    for file in FILES {
        args.extend(["--file", file]);
    }
    args
}
pub fn run() -> Result<()> {
    invalidate(Path::new("."))?;
    let args = scanner_args();
    let command = process::run("cargo", &args, Duration::from_secs(4_500))
        .map_err(|error| -> Box<dyn Error> { error.into() });
    let publication = write_report(Path::new("."), &command);
    // A secondary artifact/publication failure must not erase the original
    // typed process failure. Both classifications are recorded when writable.
    if let Ok(report) = &publication {
        println!(
            "mutation campaign: {:?}, {:?} caught, {:?} missed",
            report.verdict, report.caught, report.missed
        );
    }
    command?;
    let report = publication?;
    if report.verdict != Verdict::Pass {
        return Err(report
            .failure
            .unwrap_or_else(|| "mutation campaign failed".into())
            .into());
    }
    Ok(())
}
#[cfg(test)]
mod tests;
