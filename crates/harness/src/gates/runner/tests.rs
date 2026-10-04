//! Shared runner regression fixtures.
use super::*;
use evidence::{EndpointProof, JudgeIdentity, Metadata, PrivateOutput};

fn registry() -> Registry {
    Registry::parse(&serde_json::to_vec(&serde_json::json!({
        "version": 2,
        "suites": [
            {"id":"everything","paths":["**"],"implies":["static","game"],"review":true},
            {"id":"static","paths":["docs/**"],"implies":[],"review":false},
            {"id":"game","paths":["crates/**"],"implies":[],"review":false}
        ],
        "gates": [
            {"id":"alpha","command":"make alpha","requires":[],"select":"baseline","evidence":"log",
             "suites":["static"],"cadences":["pr","ci","preflight"],"budget_s":10,
             "static":true,"capabilities":[],"blocks":["pr","ci","preflight"]},
            {"id":"beta","command":"make beta","requires":["alpha"],"select":"game","evidence":"log",
             "suites":["game"],"cadences":["pr","ci","preflight"],"budget_s":20,
             "static":false,"capabilities":["docker"],"blocks":["pr","ci","preflight"]},
            {"id":"gamma","command":"make gamma","requires":[],"select":"game","evidence":"log",
             "suites":["game"],"cadences":["pr","ci","preflight"],"budget_s":20,
             "static":false,"capabilities":["source-assets","source-geodata","hardware"],"blocks":["pr"]}
        ],
        "jobs":{"all":["beta","gamma"],"core":["alpha"]}
    })).unwrap()).unwrap()
}
fn proof() -> EndpointProof {
    EndpointProof {
        source: "source-raw".into(),
        private: "private-raw".into(),
    }
}
fn metadata() -> Metadata {
    Metadata {
        revision: "candidate-full-oid".into(),
        base_revision: None,
        tree: Some("exact-tree".into()),
        scope: crate::gates::scopes::Kind::Index,
        index_fingerprint: Some("index-bytes".into()),
        fingerprint: proof(),
        judge: JudgeIdentity {
            executable_blake3: "judge-bytes".into(),
            policy_revision: None,
            trust_closure_hash: None,
            mode: "bootstrap-local".into(),
        },
        tool_image_actual_ids: BTreeMap::new(),
        capability_limits: vec!["fake capabilities only".into()],
        runtime_limits: vec!["no runtime hooks observed; same-user forgery possible".into()],
    }
}
struct Fake {
    now: u64,
    calls: Vec<(String, PathBuf, Duration)>,
    caps: Capabilities,
    failure: Option<Exit>,
    mutate: bool,
    private_mutation: bool,
    cancel: bool,
    missing_log: bool,
}
impl Fake {
    fn new() -> Self {
        Self {
            now: 0,
            calls: Vec::new(),
            caps: [
                Capability::Docker,
                Capability::SourceAssets,
                Capability::SourceGeodata,
                Capability::Hardware,
            ]
            .into_iter()
            .map(|cap| {
                (
                    cap,
                    CapabilityState::Available {
                        observation: "fake".into(),
                    },
                )
            })
            .collect(),
            failure: None,
            mutate: false,
            private_mutation: false,
            cancel: false,
            missing_log: false,
        }
    }
}
impl Runtime for Fake {
    fn now_ms(&self) -> u64 {
        self.now
    }
    fn cancelled(&self) -> bool {
        self.cancel
    }
    fn capabilities(&mut self) -> Capabilities {
        self.caps.clone()
    }
    fn verify(&mut self, _: &ExecutionRoot) -> Result<EndpointProof> {
        let mut observed = proof();
        if !self.calls.is_empty() {
            if self.mutate {
                observed.source = "source-changed".into();
            }
            if self.private_mutation {
                observed.private = "private-changed".into();
            }
        }
        Ok(observed)
    }
    fn make(&mut self, root: &ExecutionRoot, gate: &GateId, deadline: Duration) -> Receipt {
        self.calls
            .push((gate.as_str().into(), root.path().to_owned(), deadline));
        self.now += 5;
        Receipt {
            exit: self.failure.take().unwrap_or(Exit::Success),
            reason: "fake result".into(),
            log: (!self.missing_log).then(|| LogRef {
                path: "/private/fake.log".into(),
                blake3: "log-bytes".into(),
                truncated: false,
            }),
        }
    }
}
fn root() -> (tempfile::TempDir, ExecutionRoot) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("Makefile"), b"# fake only\n").unwrap();
    let root = ExecutionRoot::new(directory.path()).unwrap();
    (directory, root)
}
fn plan() -> PreparedPlan {
    PreparedPlan::new(
        &registry(),
        Selection::Cadence {
            cadence: Cadence::Pr,
            suites: &BTreeSet::from(["everything".into()]),
        },
    )
    .unwrap()
}
fn budgets() -> Budgets {
    Budgets {
        total: Duration::from_secs(100),
        per_gate_max: Duration::from_secs(30),
    }
}

#[test]
fn deterministic_one_result_per_gate_and_success_logs() {
    let (_directory, root) = root();
    let mut runtime = Fake::new();
    let ledger = run(&mut runtime, &root, &plan(), metadata(), budgets());
    assert_eq!(ledger.overall, Overall::Pass);
    assert!(!ledger.authoritative);
    assert_eq!(
        ledger
            .results
            .iter()
            .map(|r| r.gate.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "beta", "gamma"]
    );
    assert!(
        ledger
            .results
            .iter()
            .all(|r| r.log.is_some() && r.duration_ms == 5)
    );
    assert!(runtime.calls.iter().all(|(_, cwd, _)| cwd == root.path()));
    assert_eq!(runtime.calls[0].2, Duration::from_secs(10));
}
#[test]
fn dependency_failure_blocks_but_independent_gate_continues() {
    let (_directory, root) = root();
    let mut runtime = Fake::new();
    runtime.failure = Some(Exit::Failed);
    let ledger = run(&mut runtime, &root, &plan(), metadata(), budgets());
    assert_eq!(ledger.results[0].verdict, Verdict::Fail);
    assert_eq!(ledger.results[1].verdict, Verdict::Skipped);
    assert_eq!(ledger.results[1].blocked_by, ["alpha"]);
    assert_eq!(ledger.results[2].verdict, Verdict::Pass);
    assert_eq!(ledger.overall, Overall::Incomplete);
    assert_eq!(runtime.calls.len(), 2);
}
#[test]
fn four_capabilities_are_separate_and_absence_is_not_pass() {
    let (_directory, root) = root();
    let mut runtime = Fake::new();
    runtime.caps.clear();
    let ledger = run(&mut runtime, &root, &plan(), metadata(), budgets());
    assert_eq!(ledger.results[1].unavailable, [Capability::Docker]);
    assert_eq!(
        ledger.results[2].unavailable,
        [
            Capability::SourceAssets,
            Capability::SourceGeodata,
            Capability::Hardware
        ]
    );
    assert_eq!(ledger.results[1].verdict, Verdict::Unavailable);
    assert_eq!(ledger.overall, Overall::Incomplete);
    assert_eq!(runtime.calls.len(), 1);
}
#[test]
fn source_or_private_mutation_invalidates_even_failed_execution() {
    let (_directory, root) = root();
    for private in [false, true] {
        let mut runtime = Fake::new();
        runtime.private_mutation = private;
        runtime.mutate = !private;
        runtime.failure = Some(Exit::Failed);
        let ledger = run(&mut runtime, &root, &plan(), metadata(), budgets());
        assert_eq!(ledger.overall, Overall::Invalid);
        assert_eq!(ledger.results.len(), 3);
        assert_eq!(runtime.calls.len(), 1);
        assert!(!ledger.invalid_reasons.is_empty());
    }
}
#[test]
fn budgets_cancellation_timeout_and_missing_logs_never_succeed() {
    let (_directory, root) = root();
    let mut runtime = Fake::new();
    let mut budget = budgets();
    budget.total = Duration::from_millis(5);
    let ledger = run(&mut runtime, &root, &plan(), metadata(), budget);
    assert_eq!(runtime.calls[0].2, Duration::from_millis(5));
    assert_eq!(ledger.overall, Overall::Incomplete);
    assert_eq!(ledger.results.len(), 3);
    let mut runtime = Fake::new();
    runtime.cancel = true;
    assert_eq!(
        run(&mut runtime, &root, &plan(), metadata(), budgets()).overall,
        Overall::Incomplete
    );
    assert!(runtime.calls.is_empty());
    let mut runtime = Fake::new();
    runtime.failure = Some(Exit::Deadline);
    assert_eq!(
        run(&mut runtime, &root, &plan(), metadata(), budgets()).results[0].verdict,
        Verdict::Fail
    );
    let mut runtime = Fake::new();
    runtime.missing_log = true;
    assert_eq!(
        run(&mut runtime, &root, &plan(), metadata(), budgets()).results[0].verdict,
        Verdict::Unavailable
    );
}
#[test]
fn ci_job_uses_members_and_dependency_closure_preflight_keeps_baseline() {
    let registry = registry();
    let job = PreparedPlan::new(&registry, Selection::CiJob("all")).unwrap();
    assert_eq!(job.plan.gates, ["alpha", "beta", "gamma"]);
    assert!(PreparedPlan::new(&registry, Selection::CiJob("unknown")).is_err());
    let baseline = PreparedPlan::new(
        &registry,
        Selection::Cadence {
            cadence: Cadence::Preflight,
            suites: &BTreeSet::from(["static".into()]),
        },
    )
    .unwrap();
    assert_eq!(baseline.plan.gates, ["alpha", "beta", "gamma"]);
}
#[test]
fn ids_and_privileged_catalog_fail_closed() {
    for id in [
        "release-publish",
        "merge",
        "approve",
        "approval",
        "-n",
        "a;true",
        "a b",
        "a=b",
        "../a",
        "",
    ] {
        assert!(GateId::new(id).is_err(), "{id:?}");
    }
    let mut registry = registry();
    registry.gates[0].command = "sh -c true".into();
    assert!(PreparedPlan::new(&registry, Selection::CiJob("all")).is_err());
}
#[test]
fn canonical_hash_ignores_catalog_order_but_binds_policy() {
    let mut registry = registry();
    let original = evidence::canonical_registry_hash(&registry).unwrap();
    registry.gates.reverse();
    registry.suites.reverse();
    registry.jobs.get_mut("all").unwrap().reverse();
    assert_eq!(
        original,
        evidence::canonical_registry_hash(&registry).unwrap()
    );
    registry.gates[0].budget_s += 1;
    assert_ne!(
        original,
        evidence::canonical_registry_hash(&registry).unwrap()
    );
}
#[test]
fn private_atomic_output_is_external_and_0600_not_authentication() {
    let gate = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    assert!(PrivateOutput::new(gate.path(), &[gate.path().to_owned()]).is_err());
    let private = PrivateOutput::new(output.path(), &[gate.path().to_owned()]).unwrap();
    assert!(private.atomic("../ledger.json", b"no").is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = private.atomic("success.log", b"stdout and stderr").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"stdout and stderr");
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
