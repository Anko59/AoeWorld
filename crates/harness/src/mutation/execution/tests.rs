use super::super::tests::{artifacts, repository};
use super::*;
use crate::gates::scopes::{Kind, Snapshot};
use std::fs;

fn snapshot(root: &Path) -> Snapshot {
    Snapshot::prepare_independent(
        root,
        Kind::Commit(crate::gates::scopes::resolved_head(root).unwrap()),
    )
    .unwrap()
}
#[test]
fn fixed_operation_mapping_preserves_policy_and_local_fresh_roots() {
    let root = repository();
    let snapshot = snapshot(root.path());
    let execution = execute_with(root.path(), &snapshot, |checkout, args, environment| {
        assert_eq!(checkout, snapshot.root());
        assert!(args.contains(&"--cargo-arg=--locked"));
        assert_eq!(args[0], "mutants");
        let output = Path::new(args[5]);
        assert!(!output.starts_with(root.path()));
        assert!(!output.starts_with(checkout));
        let target = Path::new(
            environment
                .iter()
                .find(|entry| entry.0 == "CARGO_TARGET_DIR")
                .unwrap()
                .1,
        );
        assert_ne!(output, target);
        for pair in GIT_ENV {
            assert!(environment.contains(&pair));
        }
        assert!(environment.contains(&("CARGO_NET_OFFLINE", "true")));
        Ok(())
    })
    .unwrap();
    assert!(execution.command.as_ref().unwrap().is_ok());
    assert_eq!(execution.after.as_ref(), Some(&execution.before));
    execution.verify(&snapshot).unwrap();
    // A no-op mapping does not generate fake artifact evidence.
    assert!(super::super::io::Pair::open(execution.artifact_directory()).is_err());
}
#[test]
fn constructed_exit_and_real_private_git_failure_are_both_retained() {
    let root = repository();
    let snapshot = snapshot(root.path());
    let execution = execute_with(root.path(), &snapshot, |checkout, _, _| {
        fs::write(
            checkout.join(".git/added-by-candidate"),
            b"endpoint violation",
        )
        .unwrap();
        Err(ProcessError::Exit {
            program: "cargo".into(),
            code: Some(7),
            log: "local-only".into(),
        })
    })
    .unwrap();
    assert!(matches!(
        &execution.command,
        Some(Err(ProcessError::Exit { code: Some(7), .. }))
    ));
    assert!(execution.endpoint_error.is_some());
    assert!(execution.after.is_none());
    assert!(execution.verify(&snapshot).is_err());
    let report =
        super::super::publication::observe_execution(root.path(), &snapshot, &execution).unwrap();
    assert_eq!(report.command_observation.exit_code, Some(7));
    assert_eq!(report.verdict, crate::perf::Verdict::Inconclusive);
    let wire: serde_json::Value = serde_json::from_slice(
        &fs::read(root.path().join("reports/mutation/nightly.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(wire["endpoint_failure"], "IMMUTABLE_ENDPOINTS_UNAVAILABLE");
    assert_eq!(wire["artifact_failure"], "INPUT_UNAVAILABLE");
    assert_eq!(
        wire["command_observation"]["root_cause"],
        "ROOT_CAUSE_NOT_ASSESSED"
    );
}
#[test]
fn same_source_subject_and_fresh_fixture_bytes_are_observed_without_authentication() {
    let root = repository();
    let snapshot = snapshot(root.path());
    // Synthetic candidate records test the publication wiring, NOT a real tool
    // run, counters qualification or compiled cache attestation.
    let execution = execute_with(root.path(), &snapshot, |_, args, _| {
        let directory = Path::new(args[5]).join("mutants.out");
        fs::create_dir(&directory).unwrap();
        let (wire, inventory) = artifacts(30, 0);
        fs::write(
            directory.join("outcomes.json"),
            serde_json::to_vec(&wire).unwrap(),
        )
        .unwrap();
        fs::write(
            directory.join("mutants.json"),
            serde_json::to_vec(&inventory).unwrap(),
        )
        .unwrap();
        Ok(())
    })
    .unwrap();
    let report =
        super::super::publication::observe_execution(root.path(), &snapshot, &execution).unwrap();
    assert_eq!(
        report.source_identity,
        "MEASURED_LOGICAL_CONTENT_NON_AUTHORITATIVE"
    );
    assert!(!report.authoritative);
    let wire: serde_json::Value = serde_json::from_slice(
        &fs::read(root.path().join("reports/mutation/nightly.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(wire["version"], 3);
    assert_eq!(wire["before"]["digest"], wire["after"]["digest"]);
    assert_eq!(wire["before"]["digest"], execution.before.digest);
    assert_eq!(wire["execution_binding"], "UNAVAILABLE");
    assert_eq!(wire["served_build_binding"], "UNAVAILABLE");
    assert_eq!(
        wire["subject"]["kind"],
        serde_json::to_value(&snapshot.identity.kind).unwrap()
    );
    fs::write(
        snapshot.root().join("source.rs"),
        b"changed after observation",
    )
    .unwrap();
    assert!(execution.verify(&snapshot).is_err());
    let report =
        super::super::publication::observe_execution(root.path(), &snapshot, &execution).unwrap();
    assert_eq!(report.verdict, crate::perf::Verdict::Inconclusive);
    assert_eq!(report.source_identity, "UNAVAILABLE");
}
#[test]
fn failed_mapped_command_never_accepts_valid_old_worktree_artifacts() {
    let root = repository();
    let snapshot = snapshot(root.path());
    let (wire, inventory) = artifacts(30, 0);
    super::super::tests::install(root.path(), &wire, &inventory);
    let execution = execute_with(root.path(), &snapshot, |_, _, _| {
        Err(ProcessError::Deadline {
            program: "cargo".into(),
            seconds: 4500,
            log: "local".into(),
        })
    })
    .unwrap();
    execution.verify(&snapshot).unwrap();
    let report =
        super::super::publication::observe_execution(root.path(), &snapshot, &execution).unwrap();
    assert_eq!(report.command_observation.status, "DEADLINE");
    assert_eq!(
        report.artifact_failure,
        Some(super::super::publication::ArtifactFailure::InputUnavailable)
    );
    assert_eq!(report.verdict, crate::perf::Verdict::Inconclusive);
}
