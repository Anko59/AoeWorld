//! Whitelisted live measurements. Legacy ledger reasons/logs remain local and raw.
use super::{
    ExecutionRoot, GateId, Overall, Result, Runtime,
    evidence::{EndpointProof, Ledger},
};
use crate::process::{SafeErrorKind, SafeObservation};
use serde::Serialize;
use std::error::Error;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum CommandObservation {
    Measured {
        capture: SafeObservation,
        retention: Retention,
    },
    NotStarted {
        reason: Precondition,
    },
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Precondition {
    PreconditionUnavailable,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Retention {
    Retained,
    Failed { io_kind: SafeErrorKind },
}

/// Neither error messages nor raw host paths cross the summary boundary.
pub(crate) fn io_kind(error: &(dyn Error + 'static)) -> SafeErrorKind {
    error
        .downcast_ref::<std::io::Error>()
        .map_or(SafeErrorKind::OtherUnknown, |error| error.kind().into())
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Phase {
    Initial,
    PreGate,
    PostGate,
    FinalRuntime,
    FinalCliAfterImages,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum EndpointStatus {
    MatchesExpected,
    Changed,
    Unavailable,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct EndpointObservation {
    pub(crate) phase: Phase,
    pub(crate) gate: Option<GateId>,
    pub(crate) source: EndpointStatus,
    pub(crate) private: EndpointStatus,
}
impl EndpointObservation {
    pub(crate) fn valid(&self) -> bool {
        self.source == EndpointStatus::MatchesExpected
            && self.private == EndpointStatus::MatchesExpected
    }
}

/// Direct decisions on original-source/private proof, never reason-string parsing.
pub(crate) fn probe(
    runtime: &mut dyn Runtime,
    root: &ExecutionRoot,
    expected: &EndpointProof,
    phase: Phase,
    gate: Option<GateId>,
    endpoints: &mut Vec<EndpointObservation>,
    invalid: &mut Vec<String>,
) -> bool {
    let status = |matches| {
        if matches {
            EndpointStatus::MatchesExpected
        } else {
            EndpointStatus::Changed
        }
    };
    let (source, private) = match runtime.verify(root) {
        Ok(proof) => (
            status(proof.source == expected.source),
            status(proof.private == expected.private),
        ),
        Err(error) => {
            invalid.push(format!("snapshot verification: {error}"));
            (EndpointStatus::Unavailable, EndpointStatus::Unavailable)
        }
    };
    let observation = EndpointObservation {
        phase,
        gate,
        source,
        private,
    };
    let valid = observation.valid();
    if !valid && (source == EndpointStatus::Changed || private == EndpointStatus::Changed) {
        invalid.push("source/private snapshot identity changed".into());
    }
    endpoints.push(observation);
    valid
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Publication {
    Published,
    Failed { io_kind: SafeErrorKind },
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
enum CommandSummary<'a> {
    Observed { observation: &'a CommandObservation },
    Unobserved,
}
#[derive(Serialize)]
struct GateSummary<'a> {
    gate: &'a GateId,
    verdict: super::Verdict,
    command: CommandSummary<'a>,
}
#[derive(Serialize)]
struct SourceExpected<'a> {
    revision: &'a str,
    tree: &'a Option<String>,
    kind: &'static str,
}
#[derive(Serialize)]
struct RunSummary<'a> {
    schema: u16,
    authoritative: bool,
    root_cause: &'static str,
    execution_qualification: &'static str,
    served_build: &'static str,
    approval: &'static str,
    source_expected: SourceExpected<'a>,
    gates: Vec<GateSummary<'a>>,
    endpoints: &'a [EndpointObservation],
    execution_overall: Overall,
    final_publication: &'a Publication,
    overall: Overall,
}

/// Generated from this run's typed values ONLY, never read an old artifact.
pub(crate) fn summary(ledger: &Ledger, publication: &Publication) -> Result<String> {
    use crate::gates::scopes::Kind;
    let kind = match &ledger.metadata.scope {
        Kind::Working => "WORKING",
        Kind::Index => "INDEX",
        Kind::Commit(_) => "COMMIT",
    };
    let summary = RunSummary {
        schema: 1,
        authoritative: false,
        root_cause: "ROOT_CAUSE_NOT_ASSESSED",
        execution_qualification: "UNQUALIFIED_LOCAL_EXECUTION",
        served_build: "NOT_ASSESSED",
        approval: "NOT_ASSESSED",
        source_expected: SourceExpected {
            revision: &ledger.metadata.revision,
            tree: &ledger.metadata.tree,
            kind,
        },
        gates: ledger
            .results
            .iter()
            .map(|result| GateSummary {
                gate: &result.gate,
                verdict: result.verdict,
                command: result
                    .triage
                    .as_ref()
                    .map_or(CommandSummary::Unobserved, |observation| {
                        CommandSummary::Observed { observation }
                    }),
            })
            .collect(),
        endpoints: &ledger.endpoints,
        execution_overall: ledger.overall,
        final_publication: publication,
        overall: if matches!(publication, Publication::Failed { .. }) {
            Overall::Invalid
        } else {
            ledger.overall
        },
    };
    Ok(serde_json::to_string_pretty(&summary)?)
}

#[cfg(test)]
mod tests;
