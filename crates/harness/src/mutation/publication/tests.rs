use super::super::tests::{artifacts, install};
use super::*;
use serde_json::Value;
use std::fs;

#[test]
fn late_original_artifact_fd_change_forbids_publishing_previous_structural_pass() {
    let root = tempfile::tempdir().unwrap();
    let (wire, inventory) = artifacts(30, 0);
    install(root.path(), &wire, &inventory);
    let mut pair = Some(io::Pair::at(root.path()).unwrap());
    let mut report = Report::empty(Some(&Ok(())));
    report.verdict = Verdict::Pass;
    fs::write(
        root.path()
            .join(super::super::OUTPUT)
            .join("mutants.out/outcomes.json"),
        b"changed after observation",
    )
    .unwrap();
    endpoints(&mut report, &mut pair, None);
    assert_eq!(
        report.artifact_failure,
        Some(ArtifactFailure::ChangedDuringObservation)
    );
    publish(root.path(), &mut report, pair, None).unwrap();
    let machine: Value = serde_json::from_slice(
        &fs::read(root.path().join("reports/mutation/nightly.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(machine["verdict"], "INCONCLUSIVE");
    assert_eq!(machine["artifact_failure"], "CHANGED_DURING_OBSERVATION");
    assert!(
        !fs::read_to_string(root.path().join("reports/mutation/nightly.md"))
            .unwrap()
            .contains("Pass")
    );
}
#[cfg(unix)]
#[test]
fn original_constructed_process_error_and_human_publication_failure_remain_separate() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("reports/mutation")).unwrap();
    let sentinel = root.path().join("sentinel");
    fs::write(&sentinel, b"must not change").unwrap();
    std::os::unix::fs::symlink(&sentinel, root.path().join("reports/mutation/nightly.md")).unwrap();
    let command: Result<()> = Err(process::ProcessError::Exit {
        program: "secret-program".into(),
        code: Some(7),
        log: "secret-path".into(),
    }
    .into());
    let mut report = Report::empty(Some(&command));
    report.artifact_failure = Some(ArtifactFailure::InputUnavailable);
    report.endpoint_failure = Some("IMMUTABLE_ENDPOINTS_UNAVAILABLE");
    assert!(publish(root.path(), &mut report, None, None).is_err());
    assert_eq!(fs::read(sentinel).unwrap(), b"must not change");
    let machine: Value = serde_json::from_slice(
        &fs::read(root.path().join("reports/mutation/nightly.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(machine["command_observation"]["exit_code"], 7);
    assert_eq!(machine["artifact_failure"], "INPUT_UNAVAILABLE");
    assert_eq!(
        machine["endpoint_failure"],
        "IMMUTABLE_ENDPOINTS_UNAVAILABLE"
    );
    assert_eq!(
        machine["publication_failure"],
        "HUMAN_PUBLICATION_UNAVAILABLE"
    );
    assert_eq!(machine["verdict"], "INCONCLUSIVE");
    assert!(
        !serde_json::to_string(&machine["command_observation"])
            .unwrap()
            .contains("secret")
    );
}
#[test]
fn preparation_unavailable_and_pending_observation_never_manufacture_command_success() {
    let root = tempfile::tempdir().unwrap();
    invalidate(root.path()).unwrap();
    preparation_failed(root.path()).unwrap();
    let machine: Value = serde_json::from_slice(
        &fs::read(root.path().join("reports/mutation/nightly.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(machine["command_observation"]["status"], "NOT_STARTED");
    assert_eq!(
        machine["preparation_failure"],
        "PREPARATION_OR_INTEGRITY_UNAVAILABLE"
    );
    assert_eq!(machine["source_identity"], "UNAVAILABLE");
    assert_eq!(machine["before"], Value::Null);
    assert_eq!(machine["subject"], Value::Null);
    assert_eq!(machine["verdict"], "INCONCLUSIVE");
}
