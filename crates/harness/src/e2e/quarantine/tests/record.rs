use super::tests::{report, successful_report};
use super::*;
use serde_json::json;

fn failing_report() -> Value {
    let mut value = report();
    value["suites"][0]["specs"][0]["tests"][0]["results"][0]["error"] =
        json!({"message": "\u{1b}[2mexpect\u{1b}[22m(received).toBe(expected)\n\nExpected: 1"});
    value
}

/// Runs the report once with the given retry outcomes; returns the result, the calls made
/// and the lines of the flaky file.
fn run(listed: bool, retries: &[bool]) -> (Result<()>, usize, Vec<Value>) {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("gates")).unwrap();
    let today = Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .unwrap();
    let entries = if listed {
        json!([{"test":"memory.shared-surfaces", "gate":"test-e2e", "issue":"#391",
            "added":String::from_utf8(today.stdout).unwrap().trim(), "reason":"fixture"}])
    } else {
        json!([])
    };
    fs::write(
        directory.path().join("gates/flaky.json"),
        serde_json::to_vec(&entries).unwrap(),
    )
    .unwrap();
    let policy = flaky::Policy::load(directory.path()).unwrap();
    let mut journal = flaky::Journal::create(&directory.path().join("run.jsonl")).unwrap();
    let path = directory.path().join("run.flaky.jsonl");
    let mut flakes = record::Log::new(path.clone()).unwrap();
    let mut calls = 0;
    let result = resolve(
        Some(&policy),
        &mut journal,
        &mut flakes,
        &[REPORTER.into()],
        &directory.path().join("run.json"),
        |_, _| {
            calls += 1;
            match calls {
                1 => Ok((false, failing_report())),
                n if retries[n - 2] => Ok((true, successful_report())),
                _ => Ok((false, failing_report())),
            }
        },
    );
    let lines = fs::read_to_string(&path)
        .map(|text| {
            text.lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        })
        .unwrap_or_default();
    (result, calls, lines)
}

#[test]
fn a_failure_that_passes_on_the_one_retry_passes_the_run_and_is_recorded() {
    let (result, calls, lines) = run(false, &[true]);
    result.expect("a flake does not fail the run");
    assert_eq!(calls, 2);
    let [line] = lines.as_slice() else {
        panic!("one record: {lines:?}")
    };
    assert_eq!(line["test"], "memory.shared-surfaces memory [bounded]");
    assert_eq!(line["project"], "webgpu");
    assert_eq!(line["error"], "expect(received).toBe(expected)");
    assert_eq!(line["attempts"], 2);
    assert_eq!(line["quarantine"], Value::Null);
}

#[test]
fn a_failure_that_fails_the_retry_too_fails_the_run_and_records_nothing() {
    let (result, calls, lines) = run(false, &[false, true]);
    assert!(result.is_err());
    assert_eq!(calls, 2, "no second retry for an unlisted case");
    assert!(lines.is_empty());
}

#[test]
fn a_quarantined_case_keeps_its_two_reruns_and_names_its_issue_in_the_record() {
    let (result, calls, lines) = run(true, &[false, true]);
    result.expect("recovered on the third attempt");
    assert_eq!(calls, 3);
    assert_eq!(lines[0]["attempts"], 3);
    assert_eq!(lines[0]["quarantine"], "#391");
}

#[test]
fn a_timeout_is_never_retried_or_recorded() {
    let mut value = failing_report();
    value["suites"][0]["specs"][0]["tests"][0]["results"][0]["status"] = json!("timedOut");
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("gates")).unwrap();
    fs::write(directory.path().join("gates/flaky.json"), "[]").unwrap();
    let policy = flaky::Policy::load(directory.path()).unwrap();
    let mut journal = flaky::Journal::create(&directory.path().join("run.jsonl")).unwrap();
    let mut flakes = record::Log::unsaved();
    let mut calls = 0;
    let result = resolve(
        Some(&policy),
        &mut journal,
        &mut flakes,
        &[REPORTER.into()],
        &directory.path().join("run.json"),
        |_, _| {
            calls += 1;
            Ok((false, value.clone()))
        },
    );
    assert!(result.is_err());
    assert_eq!(calls, 1);
    assert!(flakes.summary().is_none());
}

#[test]
fn the_summary_lists_each_flake_with_its_project_and_error() {
    let mut log = record::Log::unsaved();
    assert!(log.summary().is_none());
    let case = &cases(&failing_report()).unwrap()[0];
    log.record(case, 2, None).unwrap();
    let text = log.summary().unwrap();
    assert!(text.contains("1 failed, then passed on retry"), "{text}");
    assert!(
        text.contains("| `memory.shared-surfaces memory [bounded]` | webgpu | 2 | expect(received).toBe(expected) |"),
        "{text}"
    );
}

#[test]
fn more_than_three_failures_are_a_broken_change_and_earn_no_unlisted_retry() {
    let mut value = failing_report();
    let first = value["suites"][0]["specs"][0].clone();
    let mut selected = vec![json!("case-a")];
    for index in 1..4 {
        let mut other = first.clone();
        let id = format!("case-{index}");
        other["id"] = json!(id);
        other["title"] = json!(format!("other {index}"));
        other["tests"][0]["annotations"][0]["description"] = json!(id);
        value["suites"][0]["specs"]
            .as_array_mut()
            .unwrap()
            .push(other);
        let evidence = &mut value["config"]["metadata"]["aoeQuarantine"];
        evidence["results"][&id] = evidence["results"]["case-a"].clone();
        selected.push(json!(id));
    }
    value["config"]["metadata"]["aoeQuarantine"]["selected"] = json!(selected);
    value["stats"]["unexpected"] = json!(4);
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("gates")).unwrap();
    fs::write(directory.path().join("gates/flaky.json"), "[]").unwrap();
    let policy = flaky::Policy::load(directory.path()).unwrap();
    let mut journal = flaky::Journal::create(&directory.path().join("run.jsonl")).unwrap();
    let mut calls = 0;
    let result = resolve(
        Some(&policy),
        &mut journal,
        &mut record::Log::unsaved(),
        &[REPORTER.into()],
        &directory.path().join("run.json"),
        |_, _| {
            calls += 1;
            Ok((false, value.clone()))
        },
    );
    assert!(result.is_err());
    assert_eq!(calls, 1);
}
