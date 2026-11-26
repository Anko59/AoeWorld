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
    // parser-fuzz, target-performance and network-soak: the jobs no push runs.
    assert!(jobs.len() >= 3, "{jobs:?}");
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
    assert!(calls.contains("<issue>\n<comment>\n<574>"), "{calls}");
    assert!(!calls.contains("<create>"));
    assert!(!calls.contains("<close>"));
    assert!(!calls.contains("<--jq>"));
    assert!(calls.contains("<--repo>\n<github.com/project/checkout>"));
    assert!(!calls.contains("wrong/override"));
    assert!(stub.bodies().contains("Nightly failures"));
    assert!(stub.bodies().contains("`parser`"));
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
    assert!(stdout.contains("filed follow-ups in #574"), "{stdout}");
    assert!(stub.calls().contains("<issue>\n<comment>\n<574>"));
    assert!(!stub.calls().contains("<create>"));
    println!(
        "CLI nightly-triage rehearsal (scripted GitHub): {stdout}ASSERT: commented on #574; no issue created"
    );
}

#[test]
fn one_comment_lists_all_failures_and_success_posts_nothing() {
    let stub = Stub::new(&serde_json::json!([]));
    triage(
        &stub.root,
        &needs(&[("first", "failure"), ("second", "failure")]),
        None,
    )
    .unwrap();
    assert_eq!(stub.calls().matches("<comment>").count(), 1);
    assert!(stub.bodies().contains("first") && stub.bodies().contains("second"));
    triage(&stub.root, &needs(&[("ok", "success")]), None).unwrap();
    assert_eq!(stub.calls().matches("<comment>").count(), 1);
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

#[test]
fn closed_nightly_issue_warns_and_cli_succeeds() {
    let mut stub = Stub::new(&serde_json::json!([]));
    stub.set("GH_VIEW_JSON", r#"{"state":"CLOSED"}"#);
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
        .env_remove("AOE_JUDGE_REV")
        .output()
        .unwrap();
    assert!(output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("warning: could not file follow-ups in #574")
            && error.contains("CLOSED")
            && error.contains("file them by hand"),
        "{error}"
    );
    assert!(!stub.calls().contains("<comment>"));
}

fn run_jobs() -> String {
    serde_json::json!({"jobs": [
        {"id": 11, "name": "parser", "conclusion": "success",
         "html_url": "https://github.com/project/checkout/actions/runs/123/job/11", "steps": []},
        {"id": 22, "name": "full-functional", "conclusion": "failure",
         "html_url": "https://github.com/project/checkout/actions/runs/123/job/22",
         "steps": [
            {"name": "Run make preflight", "number": 5, "conclusion": "success"},
            {"name": "Run make sdk-python-test", "number": 7, "conclusion": "failure"},
            {"name": "Run make test-e2e", "number": 8, "conclusion": "skipped"}
         ]}
    ]})
    .to_string()
}

const JOB_LOG: &str = "\
2026-11-26T11:40:30.1Z PASS [   0.2s] (1/9) crate a::ok\n\
2026-11-26T11:40:31.2Z         FAIL [   0.4s] (2/9) aoe-server map_worker::slow_start\n\
2026-11-26T11:40:32.3Z FAILED ai/tests/test_full_game.py::test_bots_reach_feudal - AssertionError: {'seed': 4}\n\
2026-11-26T11:40:32.4Z FAILED ai/tests/test_full_game.py::test_bots_reach_feudal - AssertionError: {'seed': 4}\n\
2026-11-26T11:40:33.5Z error: python3 exited with Some(1); log: reports/process/python3.log\n\
2026-11-26T11:40:33.6Z ##[error]Process completed with exit code 2.\n";

#[test]
fn a_failing_job_names_its_step_its_failing_tests_and_links_the_log() {
    let mut stub = Stub::new(&serde_json::json!([]));
    stub.set("GH_JOBS_JSON", &run_jobs());
    stub.set("GH_JOB_LOG", JOB_LOG);
    triage(
        &stub.root,
        &needs(&[("parser", "success"), ("full-functional", "failure")]),
        Some("123"),
    )
    .unwrap();
    let calls = stub.calls();
    assert!(
        calls.contains("<repos/project/checkout/actions/runs/123/jobs?per_page=100>"),
        "{calls}"
    );
    assert!(
        calls.contains("<repos/project/checkout/actions/jobs/22/logs>"),
        "{calls}"
    );
    assert!(
        !calls.contains("actions/jobs/11/logs"),
        "only failing jobs have their log read"
    );
    let body = stub.bodies();
    assert!(
        body.contains("- [ ] `full-functional` failed at step `Run make sdk-python-test`: https://github.com/project/checkout/actions/runs/123/job/22#step:7:1"),
        "{body}"
    );
    assert!(
        body.contains("`FAIL [   0.4s] (2/9) aoe-server map_worker::slow_start`"),
        "{body}"
    );
    assert!(body.contains("`FAILED ai/tests/test_full_game.py::test_bots_reach_feudal - AssertionError: {'seed': 4}`"), "{body}");
    assert_eq!(
        body.matches("test_bots_reach_feudal").count(),
        1,
        "a repeated line is listed once"
    );
    assert!(
        body.contains("`error: python3 exited with Some(1); log: reports/process/python3.log`"),
        "{body}"
    );
    assert!(
        !body.contains("##[error]") && !body.contains("PASS"),
        "{body}"
    );
}

#[test]
fn failure_lines_are_capped_and_cannot_break_the_markdown() {
    let log: String = (0..40)
        .map(|n| {
            format!(
                "2026-11-26T11:40:30.1Z FAILED test_{n} `x` {}\n",
                "y".repeat(400)
            )
        })
        .collect();
    let lines = failure_lines(&log);
    assert_eq!(lines.len(), FAILURE_LINES);
    for line in lines {
        assert!(
            line.chars().count() <= LINE_CHARS && !line.contains('`'),
            "{line}"
        );
    }
    assert!(failure_lines("nothing wrong here\n").is_empty());
}

#[test]
fn a_run_the_api_cannot_describe_still_gets_the_plain_line() {
    let mut stub = Stub::new(&serde_json::json!([]));
    stub.set("GH_JOBS_JSON", "not json");
    triage(&stub.root, &needs(&[("parser", "failure")]), Some("123")).unwrap();
    assert!(
        stub.bodies().contains("- [ ] `parser` failed\n"),
        "{}",
        stub.bodies()
    );
    // A job the run does not list, and a run without an id, keep the plain line too.
    stub.set("GH_JOBS_JSON", &run_jobs());
    triage(&stub.root, &needs(&[("other", "failure")]), Some("123")).unwrap();
    triage(&stub.root, &needs(&[("full-functional", "failure")]), None).unwrap();
    let bodies = stub.bodies();
    assert!(bodies.contains("- [ ] `other` failed\n"), "{bodies}");
    assert!(
        bodies.contains("- [ ] `full-functional` failed\n"),
        "{bodies}"
    );
}
