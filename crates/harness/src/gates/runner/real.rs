use super::*;
use crate::{
    gates::scopes::Snapshot,
    process::{self, Cancellation, CaptureExit},
};
use evidence::{EndpointProof, PrivateOutput};
use std::time::Instant;

pub(super) struct Local<'a> {
    pub(super) backend: cli::Backend,
    pub(super) snapshot: &'a Snapshot,
    pub(super) source: PathBuf,
    pub(super) output: &'a PrivateOutput,
    pub(super) cancellation: Cancellation,
    pub(super) started: Instant,
    pub(super) total: Duration,
}
impl Runtime for Local<'_> {
    fn now_ms(&self) -> u64 {
        self.started
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }
    fn cancelled(&self) -> bool {
        self.cancellation.cancelled()
    }
    fn capabilities(&mut self) -> Capabilities {
        if self.backend == cli::Backend::RestrictedLocal {
            let docker = if crate::gates::policy::supervisor::worker::deployment_observed() {
                CapabilityState::Available { observation: "fixed local deployment paths observed; per-operation image/daemon admission still required".into() }
            } else {
                CapabilityState::Unavailable {
                    reason: "fixed local worker deployment unavailable".into(),
                }
            };
            return BTreeMap::from([
                (Capability::Docker, docker),
                (
                    Capability::SourceAssets,
                    CapabilityState::Unavailable {
                        reason: "original assets unqualified".into(),
                    },
                ),
                (
                    Capability::SourceGeodata,
                    CapabilityState::Unavailable {
                        reason: "source geodata unqualified".into(),
                    },
                ),
                (
                    Capability::Hardware,
                    CapabilityState::Unavailable {
                        reason: "hardware unqualified".into(),
                    },
                ),
            ]);
        }
        let observed = process::capture_in(
            &self.source,
            "docker",
            &["info", "--format", "{{.ServerVersion}}"],
            &[],
            Duration::from_secs(5).min(self.total.saturating_sub(self.started.elapsed())),
            &self.cancellation,
        );
        let docker = if matches!(observed.exit, CaptureExit::Success) {
            CapabilityState::Available {
                observation: format!(
                    "live daemon {}",
                    String::from_utf8_lossy(&observed.stdout).trim()
                ),
            }
        } else {
            CapabilityState::Unavailable {
                reason: "Docker daemon not observed".into(),
            }
        };
        BTreeMap::from([
            (Capability::Docker, docker),
            (
                Capability::SourceAssets,
                CapabilityState::Unavailable {
                    reason: "no qualified original asset identity supplied".into(),
                },
            ),
            (
                Capability::SourceGeodata,
                CapabilityState::Unavailable {
                    reason: "no qualified real-source package supplied".into(),
                },
            ),
            (
                Capability::Hardware,
                CapabilityState::Unavailable {
                    reason: "no qualified target-hardware manifest supplied".into(),
                },
            ),
        ])
    }
    fn verify(&mut self, root: &ExecutionRoot) -> Result<EndpointProof> {
        if root.path() != self.snapshot.root() {
            return Err("execution root differs from prepared scope".into());
        }
        let (source, private) = self.snapshot.fingerprints()?;
        Ok(EndpointProof { source, private })
    }
    fn make(&mut self, root: &ExecutionRoot, gate: &GateId, deadline: Duration) -> Receipt {
        if self.backend == cli::Backend::RestrictedLocal {
            return restricted_fixed(
                self.snapshot,
                self.output,
                &self.cancellation,
                root,
                gate,
                deadline,
            );
        }
        make_fixed(
            &self.source,
            self.output,
            &self.cancellation,
            root,
            gate,
            deadline,
        )
    }
}

/// The source witness remains alive throughout create/start/wait/cleanup and final checks.
fn restricted_fixed(
    snapshot: &Snapshot,
    output: &PrivateOutput,
    cancel: &Cancellation,
    root: &ExecutionRoot,
    gate: &GateId,
    budget: Duration,
) -> Receipt {
    use crate::gates::policy::supervisor::worker::{self, Cleanup, Status};
    let mut execution = worker::Execution {
        observation: worker::Observation::empty(),
        logs: None,
    };
    let operation = worker::operation(gate.as_str());
    if snapshot.identity.isolated_inputs
        && root.path() == snapshot.root()
        && let (Some(operation), Ok(before)) = (operation, snapshot.content_witness())
        && snapshot.run_checked(|_| Ok(())).is_ok()
    {
        // Do not let a snapshot error mask the independently retained worker outcome.
        execution = worker::execute(
            root.path(),
            operation,
            output,
            budget,
            cancel,
            &before.digest,
        );
        let unchanged = snapshot
            .run_checked(|_| snapshot.content_witness())
            .is_ok_and(|after| after == before);
        execution.observation.source_witness_unchanged = Some(unchanged);
        if !unchanged && execution.observation.status == Status::CompletedNonAuthoritative {
            execution.observation.status = Status::Incomplete;
        }
    }
    let (log, retention) = if let Some(capture) = execution.logs {
        let mut bytes = b"--- docker logs CLI stdout tail ---\n".to_vec();
        bytes.extend(&capture.stdout);
        bytes.extend(b"\n--- docker logs CLI stderr tail ---\n");
        bytes.extend(&capture.stderr);
        match output.atomic(&format!("{}.log", gate.as_str()), &bytes) {
            Ok(path) => (
                Some(LogRef {
                    path,
                    blake3: blake3::hash(&bytes).to_hex().to_string(),
                    truncated: capture.truncated,
                }),
                triage::Retention::Retained,
            ),
            Err(error) => (
                None,
                triage::Retention::Failed {
                    io_kind: triage::io_kind(error.as_ref()),
                },
            ),
        }
    } else {
        (None, triage::Retention::Unavailable)
    };
    let observed = &execution.observation;
    let exit = match observed.status {
        Status::CompletedNonAuthoritative
            if observed.cleanup == Cleanup::VerifiedAbsent
                && observed.journal_retained
                && observed.template_endpoint_unchanged
                && observed.source_witness_unchanged == Some(true) =>
        {
            Exit::Success
        }
        Status::Failed => Exit::Failed,
        Status::Deadline => Exit::Deadline,
        Status::Cancelled => Exit::Cancelled,
        _ => Exit::MonitorError,
    };
    Receipt {
        exit,
        reason: format!(
            "restricted local worker {:?}; independent authority UNAVAILABLE",
            observed.status
        ),
        log,
        triage: Some(triage::CommandObservation::RestrictedWorker {
            observation: execution.observation,
            log_retention: retention,
        }),
    }
}

// Private fixed-program seam shared by Local and canonical in-module fixtures.
fn make_fixed(
    source: &Path,
    output: &PrivateOutput,
    cancellation: &Cancellation,
    root: &ExecutionRoot,
    gate: &GateId,
    deadline: Duration,
) -> Receipt {
    let cargo = source.join(".cache/cargo");
    let target = source.join("target");
    let Some(cargo) = cargo.to_str() else {
        return Receipt {
            exit: Exit::StartError,
            reason: "non-UTF-8 cache path".into(),
            log: None,
            triage: Some(triage::CommandObservation::NotStarted {
                reason: triage::Precondition::PreconditionUnavailable,
            }),
        };
    };
    let Some(target) = target.to_str() else {
        return Receipt {
            exit: Exit::StartError,
            reason: "non-UTF-8 target path".into(),
            log: None,
            triage: Some(triage::CommandObservation::NotStarted {
                reason: triage::Precondition::PreconditionUnavailable,
            }),
        };
    };
    let captured: process::Captured = process::capture_in(
        root.path(),
        "make",
        &["--", gate.as_str()],
        &[
            ("HARNESS_CARGO_CACHE", cargo),
            ("HARNESS_TARGET_CACHE", target),
        ],
        deadline,
        cancellation,
    );
    let capture = process::safe_observation(&captured);
    let mut bytes = b"--- stdout ---\n".to_vec();
    bytes.extend(&captured.stdout);
    bytes.extend(b"\n--- stderr ---\n");
    bytes.extend(&captured.stderr);
    let (log, retention) = match output.atomic(&format!("{}.log", gate.as_str()), &bytes) {
        Ok(path) => (
            Some(LogRef {
                path,
                blake3: blake3::hash(&bytes).to_hex().to_string(),
                truncated: captured.truncated,
            }),
            triage::Retention::Retained,
        ),
        Err(error) => (
            None,
            triage::Retention::Failed {
                io_kind: triage::io_kind(error.as_ref()),
            },
        ),
    };
    let (exit, reason) = match captured.exit {
        CaptureExit::Success => (Exit::Success, "supervised command exited zero".to_owned()),
        CaptureExit::Failed(code) => (Exit::Failed, format!("exit {code:?}")),
        CaptureExit::Deadline => (Exit::Deadline, "wall deadline exceeded".into()),
        CaptureExit::Cancelled => (Exit::Cancelled, "human cancellation".into()),
        CaptureExit::Start(error) => (Exit::StartError, format!("start: {error}")),
        CaptureExit::Monitor(error) => (Exit::MonitorError, format!("monitor: {error}")),
    };
    Receipt {
        exit,
        reason: format!("{reason}; captured {}ms", captured.duration.as_millis()),
        log,
        triage: Some(triage::CommandObservation::Measured { capture, retention }),
    }
}

#[cfg(test)]
pub(super) fn fixture_make(
    source: &Path,
    output: &PrivateOutput,
    cancellation: &Cancellation,
    root: &ExecutionRoot,
    gate: &GateId,
    deadline: Duration,
) -> Receipt {
    make_fixed(source, output, cancellation, root, gate, deadline)
}

#[cfg(test)]
mod tests;

pub(super) fn images(
    source: &Path,
    cancellation: &Cancellation,
    budget: Duration,
) -> BTreeMap<String, Option<String>> {
    let started = Instant::now();
    let mut images = BTreeMap::new();
    // Record after execution: make may have rebuilt mutable tags. These observed
    // current IDs cannot prove which ID a hostile gate actually ran (bootstrap).
    for (name, version) in [
        ("rust-tools", "1.93.1"),
        ("browser-tools", "1.63.0"),
        ("orchestrator", "1.93.1"),
        ("analysis", "0.19.4"),
        ("policy", "0.20.2"),
        ("coverage", "0.9.1"),
        ("fuzz", "nightly-2026-09-01-0.13.2"),
        ("mutation", "27.1.0"),
    ] {
        let tag = format!("aoeworld/{name}:{version}");
        let remaining = budget.saturating_sub(started.elapsed());
        if remaining.is_zero() || cancellation.cancelled() {
            images.insert(tag, None);
            continue;
        }
        let observed = process::capture_in(
            source,
            "docker",
            &["image", "inspect", "--format", "{{.Id}}", &tag],
            &[],
            Duration::from_secs(5).min(remaining),
            cancellation,
        );
        let id = String::from_utf8_lossy(&observed.stdout).trim().to_owned();
        images.insert(
            tag,
            (matches!(observed.exit, CaptureExit::Success)
                && id.starts_with("sha256:")
                && id.len() == 71)
                .then_some(id),
        );
    }
    images
}
