//! Actual closed local workers. Local Docker ownership never authenticates a judge.
mod config;
mod journal;
mod lifecycle;
#[cfg(test)]
mod tests;
mod transport;
pub(crate) use super::super::descriptor::Operation;
use crate::gates::runner::evidence::PrivateOutput;
use crate::process::{Cancellation, SafeObservation};
use serde::Serialize;
use std::{path::Path, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Status {
    Unavailable,
    CompletedNonAuthoritative,
    Failed,
    Deadline,
    Cancelled,
    Incomplete,
    Quarantined,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Cleanup {
    NotNeeded,
    VerifiedAbsent,
    Incomplete,
    Quarantined,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Phase {
    ImageInspect,
    Create,
    Recover,
    Inspect,
    Start,
    Wait,
    Logs,
    Stop,
    Kill,
    Remove,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct TransportObservation {
    pub(crate) phase: Phase,
    pub(crate) capture: SafeObservation,
}
/// Generated facts only: no deserialization, paths, messages, labels or worker claims.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Observation {
    pub(crate) authoritative: bool,
    pub(crate) independent_judge: &'static str,
    pub(crate) status: Status,
    pub(crate) workload_status: Status,
    pub(crate) container_exit_code: Option<i32>,
    pub(crate) controller_duration_ms: u64,
    pub(crate) cleanup: Cleanup,
    pub(crate) journal_retained: bool,
    pub(crate) template_endpoint_unchanged: bool,
    pub(crate) source_witness_unchanged: Option<bool>,
    pub(crate) transport: Vec<TransportObservation>,
}
impl Observation {
    pub(crate) fn empty() -> Self {
        Self {
            authoritative: false,
            independent_judge: "UNAVAILABLE",
            status: Status::Unavailable,
            workload_status: Status::Unavailable,
            container_exit_code: None,
            controller_duration_ms: 0,
            cleanup: Cleanup::NotNeeded,
            journal_retained: false,
            template_endpoint_unchanged: false,
            source_witness_unchanged: None,
            transport: vec![],
        }
    }
}
pub(crate) struct Execution {
    pub(crate) observation: Observation,
    /// Actual docker-logs CLI captured tails; not an invented container stream transcript.
    pub(crate) logs: Option<crate::process::Captured>,
}
pub(crate) fn deployment_observed() -> bool {
    config::Loaded::load().is_ok()
}
pub(crate) fn operation(gate: &str) -> Option<Operation> {
    match gate {
        "fmt-check" => Some(Operation::FmtCheck),
        "structure-check" => Some(Operation::StructureCheck),
        "architecture-check" => Some(Operation::ArchitectureCheck),
        "docs-check" => Some(Operation::DocsCheck),
        "lint" => Some(Operation::Lint),
        "test-unit" => Some(Operation::TestUnit),
        _ => None,
    }
}
/// A missing deployment template/endpoint/image fails before create; never Make fallback.
pub(crate) fn execute(
    source: &Path,
    operation: Operation,
    output: &PrivateOutput,
    budget: Duration,
    cancel: &Cancellation,
    source_seal: &str,
) -> Execution {
    lifecycle::execute(source, operation, output, budget, cancel, source_seal)
}
