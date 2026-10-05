//! Same original artifact FDs through immutable subject checks and local publication.
use super::{
    EXCLUDE, FILES, FILTER, Result, TOOL_VERSION, assess, execution::Execution, io, outcomes,
};
use crate::{
    gates::scopes::{ContentWitness, Identity, Snapshot},
    perf::Verdict,
    process,
};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum ArtifactFailure {
    InputUnavailable,
    SchemaInvalid,
    ChangedDuringObservation,
}
#[derive(Debug, Serialize)]
pub(super) struct CommandObservation {
    pub(super) status: &'static str,
    pub(super) exit_code: Option<i32>,
    root_cause: &'static str,
}
impl CommandObservation {
    pub(super) fn observe(result: &Result<()>) -> Self {
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
    fn not_started() -> Self {
        Self {
            status: "NOT_STARTED",
            exit_code: None,
            root_cause: "ROOT_CAUSE_NOT_ASSESSED",
        }
    }
}
#[derive(Serialize)]
struct WitnessSummary {
    schema: u16,
    algorithm: String,
    kind: serde_json::Value,
    digest: String,
}
impl WitnessSummary {
    fn new(witness: &ContentWitness) -> Result<Self> {
        Ok(Self {
            schema: witness.version,
            algorithm: witness.algorithm.clone(),
            kind: serde_json::to_value(&witness.kind)?,
            digest: witness.digest.clone(),
        })
    }
}
#[derive(Serialize)]
pub(super) struct Report {
    version: u16,
    assessment: &'static str,
    pub(super) authoritative: bool,
    pub(super) source_identity: &'static str,
    served_build_binding: &'static str,
    pub(super) artifact_freshness: &'static str,
    pub(super) execution_binding: &'static str,
    pub(super) root_cause: &'static str,
    subject: Option<Identity>,
    before: Option<WitnessSummary>,
    after: Option<WitnessSummary>,
    // Legacy compatibility only: now from held subject, never Git CWD.
    pub(super) revision: Option<String>,
    tool: &'static str,
    tool_version: &'static str,
    files: [&'static str; 3],
    filter: &'static str,
    excluded_mutants: &'static str,
    pub(super) mutant_set_hash: Option<String>,
    pub(super) artifacts: Option<io::Measurements>,
    pub(super) total_mutants: Option<u64>,
    pub(super) evaluated_mutants: Option<u64>,
    pub(super) caught: Option<u64>,
    pub(super) missed: Option<u64>,
    timeout: Option<u64>,
    unviable: Option<u64>,
    pub(super) verdict: Verdict,
    pub(super) failure: Option<String>,
    pub(super) command_observation: CommandObservation,
    pub(super) command_failure: Option<String>,
    pub(super) artifact_failure: Option<ArtifactFailure>,
    endpoint_failure: Option<&'static str>,
    preparation_failure: Option<&'static str>,
    publication_failure: Option<&'static str>,
}
impl Report {
    fn empty(command: Option<&Result<()>>) -> Self {
        Self {
            version: 3,
            assessment: "MUTATION_EVIDENCE_UNAVAILABLE_NON_AUTHORITATIVE",
            authoritative: false,
            source_identity: "UNAVAILABLE",
            served_build_binding: "UNAVAILABLE",
            artifact_freshness: "UNAVAILABLE",
            execution_binding: "UNAVAILABLE",
            root_cause: "ROOT_CAUSE_NOT_ASSESSED",
            subject: None,
            before: None,
            after: None,
            revision: None,
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
            failure: Some("mutation evidence unavailable".into()),
            command_observation: command
                .map_or_else(CommandObservation::not_started, CommandObservation::observe),
            command_failure: command
                .and_then(|result| result.as_ref().err().map(ToString::to_string)),
            artifact_failure: None,
            endpoint_failure: None,
            preparation_failure: None,
            publication_failure: None,
        }
    }
    fn unavailable(&mut self, reason: &str) {
        self.verdict = Verdict::Inconclusive;
        self.failure = Some(reason.into());
        self.assessment = "MUTATION_EVIDENCE_UNAVAILABLE_NON_AUTHORITATIVE";
    }
}
pub(super) fn invalidate(root: &Path) -> Result<()> {
    io::write_known(root,"nightly.json",br#"{"version":3,"verdict":"INCONCLUSIVE","assessment":"PENDING_NON_AUTHORITATIVE","authoritative":false,"source_identity":"UNAVAILABLE","artifact_freshness":"UNAVAILABLE","execution_binding":"UNAVAILABLE","root_cause":"ROOT_CAUSE_NOT_ASSESSED","command_observation":{"status":"NOT_STARTED","exit_code":null,"root_cause":"ROOT_CAUSE_NOT_ASSESSED"}}"#)?;
    io::write_known(
        root,
        "nightly.md",
        b"# Critical mutation campaign\n\nPENDING / INCONCLUSIVE / NON-AUTHORITATIVE.\n",
    )
}
pub(super) fn preparation_failed(root: &Path) -> Result<()> {
    let mut report = Report::empty(None);
    report.preparation_failure = Some("PREPARATION_OR_INTEGRITY_UNAVAILABLE");
    report.unavailable("mutation preparation or integrity precondition unavailable");
    publish(root, &mut report, None, None).map(|_| ())
}
fn endpoints(
    report: &mut Report,
    pair: &mut Option<io::Pair>,
    context: Option<(&Snapshot, &Execution)>,
) {
    if let Some(held) = pair
        && held.recheck().is_err()
    {
        report.artifact_failure = Some(ArtifactFailure::ChangedDuringObservation);
    }
    if let Some((snapshot, execution)) = context
        && execution.verify(snapshot).is_err()
    {
        report.endpoint_failure = Some("IMMUTABLE_ENDPOINTS_UNAVAILABLE");
        report.source_identity = "UNAVAILABLE";
    }
    // Recheck the SAME held FDs after the potentially longer Git/witness checks.
    if let Some(held) = pair
        && held.recheck().is_err()
    {
        report.artifact_failure = Some(ArtifactFailure::ChangedDuringObservation);
    }
    if report.artifact_failure.is_some() || report.endpoint_failure.is_some() {
        report.unavailable("mutation artifacts or immutable endpoints unavailable");
    }
}
fn publish(
    root: &Path,
    report: &mut Report,
    mut pair: Option<io::Pair>,
    context: Option<(&Snapshot, &Execution)>,
) -> Result<()> {
    endpoints(report, &mut pair, context);
    let human = format!(
        "# Critical mutation campaign\n\nLocal verdict: {:?}; source identity: {}; authoritative false; root cause NOT ASSESSED.\n\nCaught: {:?}; missed: {:?}; timed out: {:?}; unviable: {:?}; total: {:?}; evaluated: {:?}.\n",
        report.verdict,
        report.source_identity,
        report.caught,
        report.missed,
        report.timeout,
        report.unviable,
        report.total_mutants,
        report.evaluated_mutants
    );
    if let Err(error) = io::write_known(root, "nightly.md", human.as_bytes()) {
        report.publication_failure = Some("HUMAN_PUBLICATION_UNAVAILABLE");
        report.unavailable("mutation publication unavailable");
        let _ = io::write_known(root, "nightly.json", &serde_json::to_vec_pretty(report)?);
        return Err(error);
    }
    endpoints(report, &mut pair, context);
    if report.verdict == Verdict::Inconclusive {
        io::write_known(root,"nightly.md",b"# Critical mutation campaign\n\nINCONCLUSIVE / NON-AUTHORITATIVE; see machine failure classifications.\n")?;
    }
    if let Some(held) = &pair
        && report.artifact_failure.is_none()
    {
        report.mutant_set_hash = Some(held.measurements.inventory.raw_blake3.clone());
        report.artifacts = Some(io::Measurements {
            outcomes: io::Measurement {
                bytes: held.measurements.outcomes.bytes,
                raw_blake3: held.measurements.outcomes.raw_blake3.clone(),
            },
            inventory: io::Measurement {
                bytes: held.measurements.inventory.bytes,
                raw_blake3: held.measurements.inventory.raw_blake3.clone(),
            },
        });
    }
    endpoints(report, &mut pair, context);
    if let Err(error) = io::write_known(root, "nightly.json", &serde_json::to_vec_pretty(report)?) {
        report.publication_failure = Some("MACHINE_PUBLICATION_UNAVAILABLE");
        report.unavailable("mutation publication unavailable");
        let _ = io::write_known(root, "nightly.json", &serde_json::to_vec_pretty(report)?);
        let _ = io::write_known(
            root,
            "nightly.md",
            b"# Critical mutation campaign\n\nINCONCLUSIVE: publication unavailable.\n",
        );
        return Err(error);
    }
    // A change across the final local write invalidates the just-written claim too.
    endpoints(report, &mut pair, context);
    if report.endpoint_failure.is_some() || report.artifact_failure.is_some() {
        io::write_known(
            root,
            "nightly.md",
            b"# Critical mutation campaign\n\nINCONCLUSIVE: retained source or artifact changed.\n",
        )?;
        io::write_known(root, "nightly.json", &serde_json::to_vec_pretty(report)?)?;
    }
    Ok(())
}
#[cfg(test)]
pub(super) fn observe(
    root: &Path,
    command: &Result<()>,
    artifact_directory: &Path,
) -> Result<Report> {
    observe_with(
        root,
        Report::empty(Some(command)),
        command.is_ok(),
        artifact_directory,
        None,
    )
}
pub(super) fn observe_execution(
    root: &Path,
    snapshot: &Snapshot,
    execution: &Execution,
) -> Result<Report> {
    let mut report = Report::empty(None);
    if let Some(command) = &execution.command {
        report.command_observation = match command {
            Ok(()) => CommandObservation {
                status: "SUCCESS",
                exit_code: None,
                root_cause: "ROOT_CAUSE_NOT_ASSESSED",
            },
            Err(error) => {
                let (status, exit_code) = match error {
                    process::ProcessError::Exit { code, .. } => ("FAILED", *code),
                    process::ProcessError::Deadline { .. } => ("DEADLINE", None),
                    process::ProcessError::Cancelled { .. } => ("CANCELLED", None),
                    process::ProcessError::Start { .. } => ("START_UNAVAILABLE", None),
                    process::ProcessError::Monitor { .. } => ("MONITOR_UNAVAILABLE", None),
                };
                CommandObservation {
                    status,
                    exit_code,
                    root_cause: "ROOT_CAUSE_NOT_ASSESSED",
                }
            }
        };
        report.command_failure = command.as_ref().err().map(ToString::to_string);
    }
    observe_with(
        root,
        report,
        execution
            .command
            .as_ref()
            .is_some_and(|result| result.is_ok()),
        execution.artifact_directory(),
        Some((snapshot, execution)),
    )
}
fn observe_with(
    root: &Path,
    mut report: Report,
    command_succeeded: bool,
    artifact_directory: &Path,
    context: Option<(&Snapshot, &Execution)>,
) -> Result<Report> {
    if let Some((snapshot, execution)) = context {
        report.subject = Some(snapshot.identity.clone());
        report.revision = Some(match &snapshot.identity.kind {
            crate::gates::scopes::Kind::Commit(oid) => oid.clone(),
            _ => snapshot.identity.source_head.clone(),
        });
        report.before = Some(WitnessSummary::new(&execution.before)?);
        report.after = execution
            .after
            .as_ref()
            .map(WitnessSummary::new)
            .transpose()?;
        if execution.command.is_some() {
            report.artifact_freshness =
                "FRESH_LOCAL_ALLOCATION_AND_FIXED_INVOCATION_ATTEMPT_NON_AUTHORITATIVE";
        }
        if execution.verify(snapshot).is_ok() {
            report.source_identity = "MEASURED_LOGICAL_CONTENT_NON_AUTHORITATIVE";
        } else {
            report.endpoint_failure = Some("IMMUTABLE_ENDPOINTS_UNAVAILABLE");
        }
    }
    let pair = match io::Pair::open(artifact_directory) {
        Ok(pair) => Some(pair),
        Err(_) => {
            report.artifact_failure = Some(ArtifactFailure::InputUnavailable);
            None
        }
    };
    if let Some(pair) = &pair {
        match outcomes::parse(&pair.outcomes_bytes, &pair.inventory_bytes) {
            Ok(outcomes) => {
                report.total_mutants = Some(outcomes.total_mutants);
                report.evaluated_mutants = outcomes.evaluated();
                report.caught = Some(outcomes.caught);
                report.missed = Some(outcomes.missed);
                report.timeout = Some(outcomes.timeout);
                report.unviable = Some(outcomes.unviable);
                (report.verdict, report.failure) = assess(&outcomes, command_succeeded);
                report.assessment = if context.is_some() {
                    "IMMUTABLE_SOURCE_AND_MUTATION_BYTES_OBSERVED_NON_AUTHORITATIVE"
                } else {
                    "STRUCTURAL_FIXTURE_EVIDENCE_NON_AUTHORITATIVE"
                };
            }
            Err(_) => report.artifact_failure = Some(ArtifactFailure::SchemaInvalid),
        }
    }
    publish(root, &mut report, pair, context)?;
    Ok(report)
}
#[cfg(test)]
mod tests;
