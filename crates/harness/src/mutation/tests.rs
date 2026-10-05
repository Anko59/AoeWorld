use super::*;
use serde_json::{Value, json};
use std::fs;
mod io;
mod outcomes;

fn sample() -> Outcomes {
    Outcomes {
        cargo_mutants_version: TOOL_VERSION.into(),
        total_mutants: 42,
        caught: 38,
        missed: 0,
        timeout: 0,
        unviable: 4,
        success: 0,
        start_time: Some("started".into()),
        end_time: Some("completed".into()),
        outcomes: Vec::new(),
    }
}
fn phase(name: &str, status: Value) -> Value {
    json!({"phase":name,"duration":0.001,"process_status":status,"argv":["cargo","test"]})
}
fn descriptor(index: usize) -> Value {
    json!({"name":format!("{}:{index}:1: synthetic mutation",FILES[0]),"package":"aoe-harness",
        "file":FILES[0],"function":{"function_name":"compare","return_type":"",
        "span":{"start":{"line":1,"column":1},"end":{"line":100,"column":2}}},
        "span":{"start":{"line":index+1,"column":1},"end":{"line":index+1,"column":2}},
        "replacement":"","genre":"BinaryOperator"})
}
fn artifacts(caught: usize, unviable: usize) -> (Value, Value) {
    let mut inventory = Vec::new();
    let mut records = vec![
        json!({"scenario":"Baseline","summary":"Success","log_path":"log/baseline.log",
        "diff_path":null,"phase_results":[phase("Build",json!("Success")),phase("Test",json!("Success"))]}),
    ];
    for index in 0..caught + unviable {
        let mutant = descriptor(index);
        let mut entry = mutant.clone();
        entry["diff"] = json!("--- source\n+++ synthetic mutation\n");
        inventory.push(entry);
        let (summary, phases) = if index < caught {
            (
                "CaughtMutant",
                vec![
                    phase("Build", json!("Success")),
                    phase("Test", json!({"Failure":101})),
                ],
            )
        } else {
            ("Unviable", vec![phase("Build", json!({"Failure":101}))])
        };
        records.push(json!({"scenario":{"Mutant":mutant},"summary":summary,
            "log_path":format!("log/mutant-{index}.log"),"diff_path":format!("diff/mutant-{index}.diff"),
            "phase_results":phases}));
    }
    (
        json!({"cargo_mutants_version":TOOL_VERSION,"total_mutants":caught+unviable,
        "caught":caught,"missed":0,"timeout":0,"unviable":unviable,"success":0,
        "start_time":"started","end_time":"completed","outcomes":records}),
        json!(inventory),
    )
}
fn install(root: &Path, wire: &Value, inventory: &Value) {
    let output = root.join(OUTPUT).join("mutants.out");
    fs::create_dir_all(&output).unwrap();
    fs::write(
        output.join("outcomes.json"),
        serde_json::to_vec(wire).unwrap(),
    )
    .unwrap();
    fs::write(
        output.join("mutants.json"),
        serde_json::to_vec(inventory).unwrap(),
    )
    .unwrap();
}
fn parse(wire: &Value, inventory: &Value) -> Result<Outcomes> {
    super::outcomes::parse(
        &serde_json::to_vec(wire).unwrap(),
        &serde_json::to_vec(inventory).unwrap(),
    )
}

#[test]
fn scanner_actually_receives_every_reported_file() {
    let args = scanner_args();
    let files: Vec<_> = args
        .windows(2)
        .filter(|pair| pair[0] == "--file")
        .map(|pair| pair[1])
        .collect();
    assert_eq!(files, FILES);
    assert!(files.contains(&"crates/harness/src/gates/registry/mod.rs"));
    assert_eq!(
        &args[..6],
        &[
            "mutants",
            "--in-place",
            "--timeout",
            "120",
            "--output",
            OUTPUT
        ]
    );
    assert_eq!(&args[6..10], &["--re", FILTER, "--exclude-re", EXCLUDE]);
}
#[test]
fn incomplete_or_uncaught_mutations_cannot_pass() {
    assert_eq!(assess(&sample(), true).0, Verdict::Pass);
    assert_eq!(assess(&sample(), false).0, Verdict::Inconclusive);
    let mut sample = sample();
    sample.missed = 1;
    sample.caught -= 1;
    assert_eq!(assess(&sample, false).0, Verdict::Regression);
    sample.missed = 0;
    sample.timeout = 1;
    assert_eq!(assess(&sample, false).0, Verdict::Regression);
    sample.end_time = None;
    assert_eq!(assess(&sample, true).0, Verdict::Inconclusive);
    sample.end_time = Some("complete".into());
    sample.total_mutants = 1;
    assert_eq!(assess(&sample, true).0, Verdict::Inconclusive);
    sample.total_mutants = 42;
    sample.cargo_mutants_version = "wrong".into();
    assert_eq!(assess(&sample, true).0, Verdict::Inconclusive);
}
#[test]
fn missing_outcomes_yield_inconclusive_report() {
    let temp = tempfile::tempdir().unwrap();
    let command: Result<()> = Err("missing tool".into());
    let report = write_report(temp.path(), &command).unwrap();
    assert_eq!(report.verdict, Verdict::Inconclusive);
    assert_eq!(
        report.artifact_failure,
        Some(ArtifactFailure::InputUnavailable)
    );
    assert_eq!(report.command_observation.status, "FAILED_UNCLASSIFIED");
    assert!(temp.path().join("reports/mutation/nightly.json").is_file());
    assert!(temp.path().join("reports/mutation/nightly.md").is_file());
}
#[test]
fn complete_outcomes_write_revision_bound_pass_and_regression_reports() {
    let temp = tempfile::tempdir().unwrap();
    let (mut wire, inventory) = artifacts(38, 4);
    install(temp.path(), &wire, &inventory);
    let report = write_report(temp.path(), &Ok(())).unwrap();
    assert_eq!(report.verdict, Verdict::Pass);
    assert_eq!(report.total_mutants, Some(42));
    assert_eq!(report.evaluated_mutants, Some(38));
    assert!(matches!(
        report.revision.as_ref().map(String::len),
        Some(40 | 64)
    ));
    assert!(report.mutant_set_hash.is_some());
    assert_eq!(report.source_identity, "UNAVAILABLE");
    assert!(!report.authoritative);
    // AFTER-run revision observation is NOT source binding despite the old test name.
    wire["outcomes"][1]["summary"] = json!("MissedMutant");
    wire["outcomes"][1]["phase_results"][1]["process_status"] = json!("Success");
    wire["missed"] = json!(1);
    wire["caught"] = json!(37);
    install(temp.path(), &wire, &inventory);
    let report = write_report(temp.path(), &Err("mutants failed".into())).unwrap();
    assert_eq!(report.verdict, Verdict::Regression);
}
#[test]
fn typed_process_failure_is_separate_from_artifact_failure_and_never_passes_old_claims() {
    let temp = tempfile::tempdir().unwrap();
    let (wire, inventory) = artifacts(30, 0);
    install(temp.path(), &wire, &inventory);
    let command: Result<()> = Err(process::ProcessError::Exit {
        program: "secret-program".into(),
        code: Some(7),
        log: "secret-path".into(),
    }
    .into());
    let report = write_report(temp.path(), &command).unwrap();
    assert_eq!(report.verdict, Verdict::Inconclusive);
    assert_eq!(report.command_observation.status, "FAILED");
    assert_eq!(report.command_observation.exit_code, Some(7));
    let safe = serde_json::to_string(&report.command_observation).unwrap();
    assert!(!safe.contains("secret"));
    assert!(report.command_failure.as_ref().unwrap().contains("secret"));
    fs::write(
        temp.path().join(OUTPUT).join("mutants.out/outcomes.json"),
        b"{}",
    )
    .unwrap();
    let report = write_report(temp.path(), &command).unwrap();
    assert_eq!(
        report.artifact_failure,
        Some(ArtifactFailure::SchemaInvalid)
    );
    assert_eq!(report.command_observation.exit_code, Some(7));
    assert_eq!(report.root_cause, "ROOT_CAUSE_NOT_ASSESSED");
    for command in [
        process::ProcessError::Start {
            program: "secret".into(),
            source: std::io::Error::other("secret"),
        },
        process::ProcessError::Deadline {
            program: "secret".into(),
            seconds: 4500,
            log: "secret".into(),
        },
        process::ProcessError::Cancelled {
            program: "secret".into(),
            log: "secret".into(),
        },
        process::ProcessError::Monitor {
            program: "secret".into(),
            source: std::io::Error::other("secret"),
        },
    ] {
        let command: Result<()> = Err(command.into());
        let observed = CommandObservation::observe(&command);
        assert!(!serde_json::to_string(&observed).unwrap().contains("secret"));
        assert_ne!(observed.status, "SUCCESS");
    }
}
#[test]
fn malformed_artifacts_invalidate_previous_local_pass_both_publications() {
    let temp = tempfile::tempdir().unwrap();
    let (wire, inventory) = artifacts(30, 0);
    install(temp.path(), &wire, &inventory);
    assert_eq!(
        write_report(temp.path(), &Ok(())).unwrap().verdict,
        Verdict::Pass
    );
    fs::write(
        temp.path().join(OUTPUT).join("mutants.out/mutants.json"),
        b"[]",
    )
    .unwrap();
    let report = write_report(temp.path(), &Ok(())).unwrap();
    assert_eq!(report.verdict, Verdict::Inconclusive);
    let machine: Value = serde_json::from_slice(
        &fs::read(temp.path().join("reports/mutation/nightly.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(machine["verdict"], "INCONCLUSIVE");
    assert_eq!(machine["artifact_freshness"], "UNAVAILABLE");
    assert_eq!(machine["execution_binding"], "UNAVAILABLE");
    assert_eq!(machine["source_identity"], "UNAVAILABLE");
    assert!(
        !fs::read_to_string(temp.path().join("reports/mutation/nightly.md"))
            .unwrap()
            .contains("Pass")
    );
}
