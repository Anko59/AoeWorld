use super::super::test_stub::Stub;
use super::*;

fn needs(pairs: &[(&str, &str)]) -> String {
    let map: serde_json::Map<_, _> = pairs
        .iter()
        .map(|(job, result)| {
            (
                (*job).to_owned(),
                serde_json::json!({"result": result, "outputs": {}}),
            )
        })
        .collect();
    serde_json::Value::Object(map).to_string()
}

#[test]
fn the_needs_context_is_bounded_and_strict() {
    let parsed = parse(&needs(&[
        ("full-functional", "failure"),
        ("_soak", "success"),
    ]))
    .unwrap();
    assert_eq!(parsed["full-functional"], Outcome::Failure);
    assert_eq!(parsed["_soak"], Outcome::Success);
    let refused = [
        "[]".to_owned(),
        "{}".to_owned(),
        "not json".to_owned(),
        r#"{"a": {"result": "failure", "extra": 1}}"#.to_owned(),
        r#"{"a": {"result": "exploded"}}"#.to_owned(),
        r#"{"a": {}}"#.to_owned(),
        r#"{"a": {"result": "failure", "outputs": {"k": 1}}}"#.to_owned(),
        needs(&[("bad job", "failure")]),
        needs(&[("-dash", "failure")]),
        needs(&[("", "failure")]),
        needs(&[(&"j".repeat(65), "failure")]),
        format!(
            r#"{{"a": {{"result": "failure", "outputs": {{"k": "{}"}}}}}}"#,
            "x".repeat(RESULTS_LIMIT)
        ),
    ];
    for raw in &refused {
        assert!(parse(raw).is_err(), "{}", &raw[..raw.len().min(80)]);
    }
    let jobs: Vec<String> = (0..=JOB_CAP).map(|n| format!("job{n}")).collect();
    let too_many: Vec<(&str, &str)> = jobs.iter().map(|job| (job.as_str(), "success")).collect();
    assert!(parse(&needs(&too_many)).is_err());
    let outputs: serde_json::Map<_, _> = (0..=OUTPUT_CAP)
        .map(|n| (format!("k{n}"), serde_json::json!("v")))
        .collect();
    let crowded = serde_json::json!({"a": {"result": "success", "outputs": outputs}});
    assert!(parse(&crowded.to_string()).is_err());
}

#[test]
fn the_route_make_target_and_workflow_reach_the_triage() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |path: &str| std::fs::read_to_string(root.join(path)).unwrap();
    assert!(
        read(".agents/hooks/harness.sh")
            .contains("nightly-triage) probe=crates/harness/src/ship/issues/triage.rs")
    );
    let make = read("make/ship.mk");
    assert!(make.contains("nightly-triage:\n\t@.agents/hooks/harness.sh exec nightly-triage"));
    assert!(make.contains(" NIGHTLY_RESULTS"));
    let workflow = read(".github/workflows/nightly.yml");
    let (_, jobs) = workflow.split_once("\njobs:\n").unwrap();
    let (jobs, triage) = jobs.split_once("\n  triage:\n").unwrap();
    for line in [
        "if: always()",
        "issues: write",
        "run: make nightly-triage",
        "NIGHTLY_RESULTS: ${{ toJSON(needs) }}",
        "GH_TOKEN: ${{ github.token }}",
    ] {
        assert!(triage.contains(line), "{line}");
    }
    let needs = triage
        .lines()
        .find_map(|line| line.trim().strip_prefix("needs: ["))
        .unwrap();
    let jobs: Vec<_> = jobs
        .lines()
        .filter_map(|line| line.strip_prefix("  ")?.strip_suffix(':'))
        .filter(|name| !name.starts_with(' '))
        .collect();
    assert!(jobs.len() >= 5, "{jobs:?}");
    for job in jobs {
        assert!(
            needs
                .trim_end_matches(']')
                .split(", ")
                .any(|need| need == job),
            "triage does not wait for {job}"
        );
    }
}

#[test]
fn nightly_triage_concurrency_is_shared_across_refs_without_cancellation() {
    let workflow = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/nightly.yml"),
    )
    .unwrap();
    let triage = workflow.split("\n  triage:\n").nth(1).unwrap();
    assert!(
        triage
            .contains("concurrency:\n      group: nightly-triage\n      cancel-in-progress: false")
    );
    let concurrency = triage
        .split("concurrency:")
        .nth(1)
        .unwrap()
        .split("    permissions:")
        .next()
        .unwrap();
    assert!(!concurrency.contains("github.ref"));
}

#[test]
fn failing_jobs_are_recorded_on_574_without_create_or_close() {
    let stub = Stub::new(&serde_json::json!([]));
    triage(
        &stub.root,
        &needs(&[("parser", "failure"), ("ok", "success")]),
        Some("123"),
    )
    .unwrap();
    let calls = stub.calls();
    assert!(calls.contains("<issue>\n<edit>\n<574>"), "{calls}");
    assert!(!calls.contains("<create>"));
    assert!(!calls.contains("<close>"));
    assert!(!calls.contains("<--jq>"));
    assert!(calls.contains("<--repo>\n<github.com/project/checkout>"));
    assert!(!calls.contains("wrong/override"));
    assert!(stub.bodies().contains("Nightly failure: parser"));
    assert!(
        stub.bodies()
            .contains("- [ ] The nightly job `parser` failed")
    );
    assert!(stub.bodies().contains("actions/runs/123"));
}

#[test]
fn nightly_cli_uses_the_configured_issue_without_creating() {
    let stub = Stub::new(&serde_json::json!([]));
    let binary = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("aoe-harness");
    let output = std::process::Command::new(binary)
        .current_dir(&stub.root)
        .arg("nightly-triage")
        .env("NIGHTLY_RESULTS", needs(&[("parser", "failure")]))
        .env("GITHUB_RUN_ID", "123")
        .env_remove("AOE_JUDGE_REV")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("recorded parser in #574"), "{stdout}");
    assert!(stub.calls().contains("<issue>\n<edit>\n<574>"));
    assert!(!stub.calls().contains("<create>"));
    println!(
        "CLI nightly-triage rehearsal (scripted GitHub): {stdout}ASSERT: edited #574; no issue created"
    );
}

#[test]
fn failed_github_calls_are_reported_after_trying_each_failure() {
    let mut stub = Stub::new(&serde_json::json!([]));
    stub.set("GH_FAIL", "edit");
    let error = triage(
        &stub.root,
        &needs(&[("first", "failure"), ("second", "failure")]),
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("first") && error.contains("second"),
        "{error}"
    );
    assert_eq!(stub.calls().matches("<edit>").count(), 2);
}

#[test]
fn nightly_closed_target_reports_config_path() {
    let mut stub = Stub::new(&serde_json::json!([]));
    stub.set(
        "GH_VIEW_JSON",
        r#"{"state":"CLOSED","body":"","updatedAt":"t"}"#,
    );
    let error = triage(&stub.root, &needs(&[("parser", "failure")]), Some("123"))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("gates/issues.json") && error.contains("574"),
        "{error}"
    );
    assert!(!stub.calls().contains("<create>"));
    assert!(!stub.calls().contains("<edit>"));
}

#[test]
fn distinct_job_ids_keep_distinct_safe_fingerprints() {
    let stub = Stub::new(&serde_json::json!([]));
    triage(
        &stub.root,
        &needs(&[("a--b", "failure"), ("ab", "failure")]),
        None,
    )
    .unwrap();
    let body = stub.bodies();
    for job in ["a--b", "ab"] {
        assert!(body.contains(&format!(
            "aoe-workstream:nightly-job-{}:start",
            blake3::hash(job.as_bytes()).to_hex()
        )));
    }
    assert!(!body.contains("nightly-job-a--b"));
}

#[test]
fn triage_requires_the_main_session_and_results() {
    let mut stub = Stub::new(&serde_json::json!([]));
    stub.set("AOE_AGENT_ROLE", "tester");
    stub.set("NIGHTLY_RESULTS", &needs(&[("a", "failure")]));
    assert!(
        execute(&stub.root)
            .unwrap_err()
            .to_string()
            .contains("only to the main session")
    );
    stub.unset("AOE_AGENT_ROLE");
    stub.unset("NIGHTLY_RESULTS");
    assert!(
        execute(&stub.root)
            .unwrap_err()
            .to_string()
            .contains("set NIGHTLY_RESULTS")
    );
    stub.set("NIGHTLY_RESULTS", &needs(&[("a", "skipped")]));
    execute(&stub.root).unwrap();
    assert!(!stub.calls().contains("<issue>"));
}
