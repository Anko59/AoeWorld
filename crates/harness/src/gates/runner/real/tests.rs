use super::*;
use crate::gates::{policy::supervisor::worker, scopes::Kind};
use std::process::Command;
fn git(root: &Path, args: &[&str]) -> String {
    let result = Command::new("git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/var/empty")
        .current_dir(root)
        .args([
            "-c",
            "user.name=Worker Fixture",
            "-c",
            "user.email=worker@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().into()
}
#[test]
fn restricted_working_scope_and_unknown_operation_never_fallback_to_candidate_make() {
    let source = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    fs::write(
        source.path().join("Makefile"),
        "fmt-check:\n\t@touch candidate-make-ran\nunsupported:\n\t@touch candidate-make-ran\n",
    )
    .unwrap();
    git(source.path(), &["init", "--quiet", "--template="]);
    git(source.path(), &["add", "Makefile"]);
    git(source.path(), &["commit", "--quiet", "-m", "fixture"]);
    let out = PrivateOutput::new(output.path(), &[source.path().into()]).unwrap();
    let cancel = Cancellation::default();
    let working = Snapshot::prepare_independent(source.path(), Kind::Working).unwrap();
    let root = ExecutionRoot::new(working.root()).unwrap();
    let receipt = restricted_fixed(
        &working,
        &out,
        &cancel,
        &root,
        &GateId::new("fmt-check").unwrap(),
        Duration::from_secs(30),
    );
    assert!(matches!(receipt.exit, Exit::MonitorError));
    assert!(!source.path().join("candidate-make-ran").exists());
    let head = git(source.path(), &["rev-parse", "HEAD"]);
    let snapshot = Snapshot::prepare_independent(source.path(), Kind::Commit(head)).unwrap();
    let root = ExecutionRoot::new(snapshot.root()).unwrap();
    let receipt = restricted_fixed(
        &snapshot,
        &out,
        &cancel,
        &root,
        &GateId::new("unsupported").unwrap(),
        Duration::from_secs(30),
    );
    assert!(matches!(receipt.exit, Exit::MonitorError));
    assert!(!snapshot.root().join("candidate-make-ran").exists());
    let Some(triage::CommandObservation::RestrictedWorker {
        observation,
        log_retention,
    }) = receipt.triage
    else {
        panic!("missing worker fact")
    };
    assert_eq!(observation.status, worker::Status::Unavailable);
    assert!(observation.transport.is_empty());
    assert_eq!(observation.container_exit_code, None);
    assert!(matches!(log_retention, triage::Retention::Unavailable));
}
#[test]
fn safe_worker_summary_keeps_original_failure_and_independent_planes_without_paths_or_claims() {
    use crate::gates::runner::tests::{metadata, registry};
    struct Fixture {
        worker: worker::Observation,
    }
    impl Runtime for Fixture {
        fn now_ms(&self) -> u64 {
            0
        }
        fn cancelled(&self) -> bool {
            false
        }
        fn capabilities(&mut self) -> Capabilities {
            BTreeMap::new()
        }
        fn verify(&mut self, _: &ExecutionRoot) -> Result<EndpointProof> {
            Ok(metadata().fingerprint)
        }
        fn make(&mut self, _: &ExecutionRoot, _: &GateId, _: Duration) -> Receipt {
            Receipt {
                exit: Exit::Failed,
                reason: "SECRET /private/untrusted payload".into(),
                log: None,
                triage: Some(triage::CommandObservation::RestrictedWorker {
                    observation: self.worker.clone(),
                    log_retention: triage::Retention::Failed {
                        io_kind: process::SafeErrorKind::OtherUnknown,
                    },
                }),
            }
        }
    }
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Makefile"), "").unwrap();
    let plan = PreparedPlan::new(&registry(), Selection::CiJob("core")).unwrap();
    let mut observation = worker::Observation::empty();
    observation.status = worker::Status::Failed;
    observation.workload_status = worker::Status::Failed;
    observation.container_exit_code = Some(7);
    observation.cleanup = worker::Cleanup::Incomplete;
    let mut fixture = Fixture {
        worker: observation,
    };
    let ledger = run(
        &mut fixture,
        &ExecutionRoot::new(root.path()).unwrap(),
        &plan,
        metadata(),
        Budgets {
            total: Duration::from_secs(10),
            per_gate_max: Duration::from_secs(1),
        },
    );
    let summary = triage::summary(
        &ledger,
        &triage::Publication::Failed {
            io_kind: process::SafeErrorKind::OtherUnknown,
        },
    )
    .unwrap();
    assert!(!summary.contains("SECRET"));
    assert!(!summary.contains("/private"));
    let value: serde_json::Value = serde_json::from_str(&summary).unwrap();
    assert_eq!(value["authoritative"], false);
    assert_eq!(value["execution_overall"], "FAIL");
    assert_eq!(value["overall"], "INVALID");
    let worker = &value["gates"][0]["command"]["observation"]["observation"];
    assert_eq!(worker["container_exit_code"], 7);
    assert_eq!(worker["cleanup"], "INCOMPLETE");
    assert_eq!(worker["independent_judge"], "UNAVAILABLE");
}
