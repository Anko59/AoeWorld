//! Disposable fixed-Make fixtures; mock capabilities/endpoints are not attestations.
use super::*;
use crate::gates::registry::Capability;
use crate::gates::runner::{
    self, Budgets, Capabilities, CapabilityState, Exit, PreparedPlan, Receipt, Selection, Verdict,
    evidence::PrivateOutput, real,
};
use crate::process::Cancellation;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn plan(independent: bool) -> PreparedPlan {
    let mut registry = runner::tests::registry();
    registry
        .gates
        .iter_mut()
        .for_each(|gate| gate.capabilities.clear());
    registry.jobs.insert(
        "triage".into(),
        if independent {
            vec!["alpha".into(), "gamma".into()]
        } else {
            vec!["alpha".into()]
        },
    );
    PreparedPlan::new(&registry, Selection::CiJob("triage")).unwrap()
}
fn budget() -> Budgets {
    Budgets {
        total: Duration::from_secs(5),
        per_gate_max: Duration::from_secs(1),
    }
}
struct Fixture<'a> {
    output: &'a PrivateOutput,
    source: PathBuf,
    started: Instant,
    cancellation: Cancellation,
    probes: usize,
    changed_at: Option<usize>,
    private_changed: bool,
    unavailable_at: Option<usize>,
    start_failure: bool,
    deadline: bool,
}
impl Runtime for Fixture<'_> {
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis().try_into().unwrap()
    }
    // Adapter cancellation is intentionally not scheduler cancellation in this
    // fixture: the real fixed capture independently observes its own token.
    fn cancelled(&self) -> bool {
        false
    }
    fn capabilities(&mut self) -> Capabilities {
        [(
            Capability::Docker,
            CapabilityState::Available {
                observation: "MOCK".into(),
            },
        )]
        .into()
    }
    fn verify(&mut self, _: &ExecutionRoot) -> runner::Result<EndpointProof> {
        let current = self.probes;
        self.probes += 1;
        if self.unavailable_at == Some(current) {
            return Err(std::io::Error::other("ENDPOINT_SECRET /private/path").into());
        }
        let mut proof = runner::tests::metadata().fingerprint;
        if self.changed_at == Some(current) {
            if self.private_changed {
                proof.private = "changed".into();
            } else {
                proof.source = "changed".into();
            }
        }
        Ok(proof)
    }
    fn make(&mut self, root: &ExecutionRoot, gate: &GateId, deadline: Duration) -> Receipt {
        let missing = ExecutionRoot(root.path().join("missing-start-cwd"));
        real::fixture_make(
            &self.source,
            self.output,
            &self.cancellation,
            if self.start_failure { &missing } else { root },
            gate,
            if self.deadline {
                Duration::from_millis(25)
            } else {
                deadline
            },
        )
    }
}
fn fixture<'a>(source: &Path, output: &'a PrivateOutput) -> Fixture<'a> {
    Fixture {
        output,
        source: source.into(),
        started: Instant::now(),
        cancellation: Cancellation::default(),
        probes: 0,
        changed_at: None,
        private_changed: false,
        unavailable_at: None,
        start_failure: false,
        deadline: false,
    }
}
fn root(recipe: &str) -> (tempfile::TempDir, ExecutionRoot) {
    let owner = tempfile::tempdir().unwrap();
    fs::write(
        owner.path().join("Makefile"),
        format!("alpha:\n\t@{recipe}\ngamma:\n\t@printf 'later independent success\\n'\n"),
    )
    .unwrap();
    let root = ExecutionRoot::new(owner.path()).unwrap();
    (owner, root)
}
fn safe(ledger: &Ledger, publication: &Publication) -> serde_json::Value {
    serde_json::from_str(&summary(ledger, publication).unwrap()).unwrap()
}
fn capture(value: &serde_json::Value) -> &serde_json::Value {
    &value["gates"][0]["command"]["observation"]["capture"]
}

#[test]
fn actual_make_numeric_failure_raw_tails_and_later_success_are_independent() {
    let (owner, root) = root("printf 'OUT_SECRET panic-looking'; printf 'ERR_SECRET' >&2; exit 7");
    let destination = tempfile::tempdir().unwrap();
    let output = PrivateOutput::new(destination.path(), &[owner.path().into()]).unwrap();
    let ledger = runner::run(
        &mut fixture(owner.path(), &output),
        &root,
        &plan(true),
        runner::tests::metadata(),
        budget(),
    );
    assert_eq!(ledger.results[0].verdict, Verdict::Fail);
    assert_eq!(ledger.results[1].verdict, Verdict::Pass);
    let value = safe(&ledger, &Publication::Published);
    assert_eq!(capture(&value)["outcome"]["kind"], "FAILED");
    assert_eq!(capture(&value)["outcome"]["code"], 2); // GNU make, NOT recipe's 7.
    assert_eq!(capture(&value)["root_cause"], "ROOT_CAUSE_NOT_ASSESSED");
    assert_eq!(capture(&value)["stdout"]["bytes"], 24);
    assert_eq!(
        capture(&value)["stdout"]["raw_blake3"],
        blake3::hash(b"OUT_SECRET panic-looking")
            .to_hex()
            .to_string()
    );
    let log = fs::read(destination.path().join("alpha.log")).unwrap();
    assert!(String::from_utf8_lossy(&log).contains("ERR_SECRET"));
    let json = summary(&ledger, &Publication::Published).unwrap();
    for secret in [
        "OUT_SECRET",
        "ERR_SECRET",
        "panic-looking",
        "/private/fake.log",
        "judge-bytes",
    ] {
        assert!(!json.contains(secret));
    }
    assert_eq!(
        value["gates"][1]["command"]["observation"]["capture"]["outcome"]["kind"],
        "SUCCESS"
    );
}

mod product;

#[test]
fn direct_endpoint_phases_preserve_source_private_and_bounded_error_absence() {
    for private in [false, true] {
        let (owner, root) = root("exit 7");
        let destination = tempfile::tempdir().unwrap();
        let output = PrivateOutput::new(destination.path(), &[owner.path().into()]).unwrap();
        let mut runtime = fixture(owner.path(), &output);
        runtime.changed_at = Some(2);
        runtime.private_changed = private;
        let mut ledger = runner::run(
            &mut runtime,
            &root,
            &plan(false),
            runner::tests::metadata(),
            budget(),
        );
        assert_eq!(ledger.endpoints.len(), 4);
        assert!(matches!(ledger.endpoints[0].phase, Phase::Initial));
        assert!(matches!(ledger.endpoints[1].phase, Phase::PreGate));
        assert!(matches!(ledger.endpoints[2].phase, Phase::PostGate));
        assert!(matches!(ledger.endpoints[3].phase, Phase::FinalRuntime));
        let post = &ledger.endpoints[2];
        assert_eq!(
            post.source,
            if private {
                EndpointStatus::MatchesExpected
            } else {
                EndpointStatus::Changed
            }
        );
        assert_eq!(
            post.private,
            if private {
                EndpointStatus::Changed
            } else {
                EndpointStatus::MatchesExpected
            }
        );
        runtime.unavailable_at = Some(4);
        assert!(!probe(
            &mut runtime,
            &root,
            &ledger.metadata.fingerprint,
            Phase::FinalCliAfterImages,
            None,
            &mut ledger.endpoints,
            &mut ledger.invalid_reasons
        ));
        let json = summary(&ledger, &Publication::Published).unwrap();
        assert!(
            ledger
                .invalid_reasons
                .iter()
                .any(|reason| reason.contains("ENDPOINT_SECRET"))
        );
        assert!(!json.contains("ENDPOINT_SECRET"));
        assert!(!json.contains("/private/path"));
        assert!(json.contains("FINAL_CLI_AFTER_IMAGES"));
    }
}

#[test]
fn actual_publication_collision_never_reads_or_replaces_forged_pass_fallback() {
    let (owner, root) = root("printf 'PUBLISH_SECRET'");
    let destination = tempfile::tempdir().unwrap();
    let forged = destination.path().join("ledger.json");
    fs::create_dir(&forged).unwrap();
    fs::write(forged.join("old-pass"), b"{\"overall\":\"PASS\"}").unwrap();
    let output = PrivateOutput::new(destination.path(), &[owner.path().into()]).unwrap();
    let ledger = runner::run(
        &mut fixture(owner.path(), &output),
        &root,
        &plan(false),
        runner::tests::metadata(),
        budget(),
    );
    assert_eq!(ledger.overall, Overall::Pass);
    assert!(runner::cli::publish(&output, &ledger).is_err());
    let error = output.ledger(&ledger).unwrap_err();
    let value = safe(
        &ledger,
        &Publication::Failed {
            io_kind: io_kind(error.as_ref()),
        },
    );
    assert_eq!(capture(&value)["outcome"]["kind"], "SUCCESS");
    assert_eq!(value["execution_overall"], "PASS");
    assert_eq!(value["overall"], "INVALID");
    assert_eq!(
        fs::read(forged.join("old-pass")).unwrap(),
        b"{\"overall\":\"PASS\"}"
    );
}

#[cfg(unix)]
#[test]
fn non_utf_cache_precondition_has_no_fabricated_capture_or_retention() {
    use std::os::unix::ffi::OsStringExt;
    let (owner, root) = root("true");
    let destination = tempfile::tempdir().unwrap();
    let output = PrivateOutput::new(destination.path(), &[owner.path().into()]).unwrap();
    let source = PathBuf::from(std::ffi::OsString::from_vec(vec![0xff]));
    let receipt = real::fixture_make(
        &source,
        &output,
        &Cancellation::default(),
        &root,
        &GateId::new("alpha").unwrap(),
        Duration::from_secs(1),
    );
    assert!(matches!(receipt.exit, Exit::StartError));
    let value = serde_json::to_value(receipt.triage.unwrap()).unwrap();
    assert_eq!(value["kind"], "NOT_STARTED");
    assert_eq!(value["reason"], "PRECONDITION_UNAVAILABLE");
    assert!(value.get("capture").is_none());
    assert!(value.get("retention").is_none());
    let ledger = runner::run(
        &mut fixture(&source, &output),
        &root,
        &plan(false),
        runner::tests::metadata(),
        budget(),
    );
    let value = safe(&ledger, &Publication::Published);
    assert_eq!(
        value["gates"][0]["command"]["observation"]["kind"],
        "NOT_STARTED"
    );
    assert!(capture(&value).is_null());
    assert_ne!(value["overall"], "PASS");
}

#[test]
fn both_stream_byte_variations_bound_raw_tails_and_closed_summary_fields() {
    for byte in ["x", "y"] {
        let (owner, root) = root(&format!(
            "head -c 70000 /dev/zero | tr '\\000' '{byte}'; head -c 70000 /dev/zero | tr '\\000' '{byte}' >&2"
        ));
        let destination = tempfile::tempdir().unwrap();
        let output = PrivateOutput::new(destination.path(), &[owner.path().into()]).unwrap();
        let ledger = runner::run(
            &mut fixture(owner.path(), &output),
            &root,
            &plan(false),
            runner::tests::metadata(),
            budget(),
        );
        let value = safe(&ledger, &Publication::Published);
        let measured = capture(&value);
        assert_eq!(measured["outcome"]["kind"], "SUCCESS");
        assert_eq!(measured["truncated"], true);
        let bytes = vec![byte.as_bytes()[0]; 65536];
        for stream in ["stdout", "stderr"] {
            assert_eq!(measured[stream]["bytes"], 65536);
            assert_eq!(
                measured[stream]["raw_blake3"],
                blake3::hash(&bytes).to_hex().to_string()
            );
            let keys: std::collections::BTreeSet<_> = measured[stream]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(keys, ["bytes", "raw_blake3"].into());
        }
        let keys: std::collections::BTreeSet<_> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "schema",
                "authoritative",
                "root_cause",
                "execution_qualification",
                "served_build",
                "approval",
                "source_expected",
                "gates",
                "endpoints",
                "execution_overall",
                "final_publication",
                "overall"
            ]
            .into()
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn incomplete_eof_marks_aggregate_truncation_without_changing_success() {
    // Escape the supervised group so its normal cleanup cannot close these
    // inherited descriptors. Kill only the fixture's distinct session below.
    let (owner, root) = root(
        "setsid sh -c 'echo $$$$ > holder.pid; sleep 5' & until [ -s holder.pid ]; do sleep 0.01; done; printf 'incomplete-eof'",
    );
    let destination = tempfile::tempdir().unwrap();
    let output = PrivateOutput::new(destination.path(), &[owner.path().into()]).unwrap();
    let ledger = runner::run(
        &mut fixture(owner.path(), &output),
        &root,
        &plan(false),
        runner::tests::metadata(),
        budget(),
    );
    use nix::{
        sys::signal::{Signal, killpg},
        unistd::{Pid, getpgid, getpgrp},
    };
    let pid = Pid::from_raw(
        fs::read_to_string(owner.path().join("holder.pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap(),
    );
    assert!(pid.as_raw() > 1);
    assert_ne!(pid, getpgrp());
    if getpgid(Some(pid)).ok() == Some(pid) {
        let _ = killpg(pid, Signal::SIGKILL);
    }
    let value = safe(&ledger, &Publication::Published);
    assert_eq!(capture(&value)["outcome"]["kind"], "SUCCESS");
    assert_eq!(capture(&value)["truncated"], true);
    assert_eq!(capture(&value)["stdout"]["bytes"], 14);
}

#[test]
fn missing_make_fixed_capture_fixture_is_actual_start_unavailable() {
    let (owner, root) = root("true");
    let empty_path = owner.path().join("missing-bin");
    let captured = crate::process::capture_in(
        root.path(),
        "make",
        &["--", "alpha"],
        &[("PATH", empty_path.to_str().unwrap())],
        Duration::from_secs(1),
        &Cancellation::default(),
    );
    let value = serde_json::to_value(crate::process::safe_observation(&captured)).unwrap();
    assert_eq!(value["outcome"]["kind"], "START");
    assert_eq!(value["outcome"]["io_kind"], "NOT_FOUND");
    assert_eq!(value["stdout"]["bytes"], 0);
    assert!(value["duration_ms"].is_number());
}

#[test]
fn synthetic_monitor_io_mapping_only_is_not_a_live_runner_receipt() {
    let captured = crate::process::Captured {
        exit: crate::process::CaptureExit::Monitor(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "MONITOR_SECRET",
        )),
        stdout: Vec::new(),
        stderr: Vec::new(),
        truncated: false,
        duration: Duration::ZERO,
    };
    // Mapping-only synthetic input: never attached to a Runtime/ledger as measured.
    let value = serde_json::to_value(crate::process::safe_observation(&captured)).unwrap();
    assert_eq!(
        value["outcome"],
        serde_json::json!({"kind":"MONITOR","io_kind":"BROKEN_PIPE"})
    );
    assert_eq!(value["root_cause"], "ROOT_CAUSE_NOT_ASSESSED");
    assert!(!value.to_string().contains("MONITOR_SECRET"));
}

#[test]
fn error_kind_downcast_never_exports_text() {
    let error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "ERROR_SECRET");
    assert_eq!(io_kind(&error), SafeErrorKind::PermissionDenied);
    let other: Box<dyn std::error::Error> = "ERROR_SECRET".into();
    assert_eq!(io_kind(other.as_ref()), SafeErrorKind::OtherUnknown);
}
