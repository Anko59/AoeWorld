use super::{Options, playwright_args, report, resolve, split_line, verdict};
use crate::e2e::quarantine::REPORTER;
use serde_json::{Value, json};

fn options(repeat: &str) -> Result<Options, String> {
    Options::parse("tests/a.spec.ts", "webgpu", repeat)
}

#[test]
fn the_count_defaults_to_thirty_and_stays_between_one_and_two_hundred() {
    assert_eq!(options("").unwrap().repeat, 30);
    assert_eq!(options("200").unwrap().repeat, 200);
    for bad in ["0", "201", "-1", "3x", "1e1"] {
        assert!(options(bad).is_err(), "{bad}");
    }
}

#[test]
fn specs_and_projects_are_required_and_projects_are_plain_names() {
    assert!(Options::parse("", "webgpu", "5").is_err());
    assert!(Options::parse("tests/a.spec.ts", "", "5").is_err());
    assert!(Options::parse("tests/a.spec.ts", "web gpu;rm", "5").is_err());
    let both = Options::parse(
        "tests/a.spec.ts tests/b.spec.ts",
        "webgpu,browser-defaults",
        "5",
    )
    .unwrap();
    assert_eq!(both.specs.len(), 2);
    assert_eq!(both.projects, ["webgpu", "browser-defaults"]);
}

#[test]
fn a_line_suffix_is_split_off_only_when_it_is_digits() {
    assert_eq!(
        split_line("tests/a.spec.ts:66"),
        ("tests/a.spec.ts", Some("66"))
    );
    assert_eq!(split_line("tests/a.spec.ts"), ("tests/a.spec.ts", None));
    assert_eq!(split_line("tests/a.spec.ts:x"), ("tests/a.spec.ts:x", None));
}

#[test]
fn a_line_keeps_to_one_literal_file() {
    let candidates = ["tests/a.spec.ts".to_owned(), "tests/b.spec.ts".to_owned()];
    assert_eq!(
        resolve(&candidates, &["tests/a.spec.ts:66".into()]).unwrap(),
        ["tests/a.spec.ts:66"]
    );
    assert_eq!(
        resolve(&candidates, &["tests/*.spec.ts".into()])
            .unwrap()
            .len(),
        2
    );
    assert!(resolve(&candidates, &["tests/*.spec.ts:66".into()]).is_err());
    assert!(resolve(&candidates, &["tests/c.spec.ts".into()]).is_err());
}

#[test]
fn the_command_line_swaps_the_reporter_and_adds_repeat_and_projects() {
    let base = ["run", "--", REPORTER, "tests/a.spec.ts"]
        .map(str::to_owned)
        .to_vec();
    let args = playwright_args(
        base,
        &Options::parse("tests/a.spec.ts", "webgpu,canvas", "7").unwrap(),
    )
    .unwrap();
    assert_eq!(
        args,
        [
            "run",
            "--",
            "--reporter=json",
            "tests/a.spec.ts",
            "--repeat-each=7",
            "--project=webgpu",
            "--project=canvas"
        ]
    );
    assert!(playwright_args(vec!["run".into()], &options("5").unwrap()).is_err());
}

fn attempt(status: &str, start: &str, message: &str) -> Value {
    json!({"status": status, "startTime": start, "duration": 5,
           "errors": [{"message": message}], "error": {"message": message}})
}

/// A spec of `file` line 66 with one test entry per attempt, as the JSON reporter writes
/// repeated tests.
fn report_of(attempts: &[(&str, &str, &str)]) -> Value {
    let tests: Vec<Value> = attempts
        .iter()
        .map(|(status, start, message)| {
            json!({"projectName": "webgpu", "results": [attempt(status, start, message)]})
        })
        .collect();
    json!({
        "errors": [],
        "suites": [{"title": "far.spec.ts", "specs": [], "suites": [{
            "title": "group", "specs": [{
                "title": "builds", "file": "tests/far.spec.ts", "line": 66, "tests": tests
            }]
        }]}]
    })
}

#[test]
fn passes_and_failures_are_counted_per_test_in_start_order() {
    let report = report_of(&[
        ("passed", "2026-01-01T00:00:03Z", ""),
        (
            "failed",
            "2026-01-01T00:00:01Z",
            "\u{1b}[2mexpect\u{1b}[22m(a)\nsecond line",
        ),
        (
            "timedOut",
            "2026-01-01T00:00:02Z",
            "Test timeout of 30000ms exceeded.",
        ),
        ("passed", "2026-01-01T00:00:04Z", ""),
    ]);
    let summary = report::summarize(&report, 4, Some("abc".into())).unwrap();
    let [test] = summary.tests.as_slice() else {
        panic!("one test")
    };
    assert_eq!((test.passed, test.failed, test.skipped), (2, 2, 0));
    assert_eq!(test.sequence, "FT..");
    assert_eq!(test.title, "group builds");
    assert_eq!(test.errors[0], "run 1: expect(a)");
    assert_eq!(test.errors[1], "run 2: Test timeout of 30000ms exceeded.");
    assert_eq!((summary.passed(), summary.failed()), (2, 2));
    assert!(summary.incomplete().is_none());
    assert!(
        verdict(&summary)
            .unwrap_err()
            .to_string()
            .contains("2 of 4")
    );
    let text = summary.markdown();
    assert!(text.contains("Passed 2, failed 2"), "{text}");
    assert!(
        text.contains("| `tests/far.spec.ts:66` group builds | webgpu | 2 | 2 | 2 | `FT..` |"),
        "{text}"
    );
}

#[test]
fn only_all_passing_attempts_make_the_run_pass() {
    let start = |n: u32| format!("2026-01-01T00:00:0{n}Z");
    let attempts: Vec<(&str, String, &str)> = (1..=3).map(|n| ("passed", start(n), "")).collect();
    let borrowed: Vec<(&str, &str, &str)> = attempts
        .iter()
        .map(|(s, t, m)| (*s, t.as_str(), *m))
        .collect();
    let summary = report::summarize(&report_of(&borrowed), 3, None).unwrap();
    verdict(&summary).expect("three of three");
    assert_eq!(summary.tests[0].sequence, "...");
}

#[test]
fn a_run_with_fewer_attempts_than_asked_proves_nothing() {
    let report = report_of(&[("passed", "2026-01-01T00:00:01Z", "")]);
    let summary = report::summarize(&report, 3, None).unwrap();
    let reason = summary.incomplete().expect("incomplete");
    assert!(reason.contains("has 1 attempts, expected 3"), "{reason}");
    assert!(verdict(&summary).is_err());
}

#[test]
fn a_run_where_nothing_ran_proves_nothing() {
    let summary = report::summarize(&json!({"errors": [], "suites": []}), 3, None).unwrap();
    assert!(summary.incomplete().unwrap().contains("no test ran"));
    let skipped = report_of(&[("skipped", "2026-01-01T00:00:01Z", "")]);
    let summary = report::summarize(&skipped, 1, None).unwrap();
    assert!(summary.incomplete().unwrap().contains("no test ran"));
}

#[test]
fn a_skipped_test_beside_a_passing_one_proves_nothing() {
    let mut report = report_of(&[("passed", "2026-01-01T00:00:01Z", "")]);
    let spec = report["suites"][0]["suites"][0]["specs"][0].clone();
    let mut skipped = spec;
    skipped["title"] = "skipped".into();
    skipped["tests"][0]["results"][0]["status"] = "skipped".into();
    report["suites"][0]["suites"][0]["specs"]
        .as_array_mut()
        .unwrap()
        .push(skipped);
    let summary = report::summarize(&report, 1, None).unwrap();
    assert!(
        summary
            .incomplete()
            .unwrap()
            .contains("group skipped [webgpu] never ran")
    );
}

#[test]
fn a_report_with_global_errors_is_refused() {
    let report = json!({"errors": [{"message": "Cannot find module"}], "suites": []});
    let error = report::summarize(&report, 3, None).unwrap_err();
    assert!(error.contains("Cannot find module"), "{error}");
    assert!(report::summarize(&json!({"errors": []}), 3, None).is_err());
}

fn workflow() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.github/workflows/e2e-repeat.yml");
    std::fs::read_to_string(path).expect("the e2e-repeat workflow")
}

#[test]
fn the_workflow_runs_only_when_dispatched_and_leaves_the_logic_to_make() {
    let text = workflow();
    let on = text.split("\npermissions:").next().unwrap();
    assert!(on.contains("workflow_dispatch:"));
    for trigger in ["pull_request", "push:", "schedule:", "workflow_run:"] {
        assert!(!on.contains(trigger), "{trigger}");
    }
    assert!(text.contains("run: make test-e2e-repeat"));
    // Inputs reach the run through the environment, never through shell text.
    for line in text
        .lines()
        .filter(|line| line.trim_start().starts_with("run:"))
    {
        assert!(!line.contains("${{"), "{line}");
    }
}
