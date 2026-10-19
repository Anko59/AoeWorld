use super::*;
use serde_json::json;

fn report() -> serde_json::Value {
    json!({"config":{"rootDir":"/checkout/browser/tests"}, "errors":[], "suites":[{"title":"memory.shared-surfaces.spec.ts", "specs":[{
        "title":"memory [bounded]", "file":"memory.shared-surfaces.spec.ts", "line":27,
        "tests":[{"projectName":"webgpu", "status":"unexpected", "expectedStatus":"passed",
            "results":[{"status":"failed"}]}]
    }]}], "stats":{"expected":0,"unexpected":1,"flaky":0,"skipped":0}})
}

#[test]
fn flaky_playwright_failure_selects_one_case_project_and_escaped_title() {
    let failures = failures(&report()).unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].key, "memory.shared-surfaces");
    // Literal flags and the full path exclude case variants and namesakes in other folders.
    assert_eq!(
        failures[0].selector(),
        [
            "/^/checkout/browser/tests/memory\\.shared-surfaces\\.spec\\.ts$/:27",
            "--project=webgpu",
            "--grep=/memory \\[bounded\\]$/"
        ]
    );
    let args = retry_args(
        &[
            "run".into(),
            "npm".into(),
            "--silent".into(),
            "run".into(),
            "test:e2e".into(),
            "--".into(),
            "--reporter=json".into(),
            "--shard=2/3".into(),
        ],
        &failures[0],
    )
    .unwrap();
    assert!(!args.iter().any(|arg| arg.starts_with("--shard")));
    assert!(args.ends_with(&failures[0].selector()));
}

#[test]
fn flaky_playwright_requires_complete_reports_and_rejects_infrastructure_errors() {
    let mut value = report();
    value["stats"]["unexpected"] = json!(2);
    assert!(failures(&value).is_err());
    let mut value = report();
    value["errors"] = json!([{"message":"worker crashed"}]);
    assert!(failures(&value).is_err());
    let mut value = report();
    value["suites"][0]["specs"][0]["tests"][0]["results"] = json!([]);
    assert!(failures(&value).is_err());
    assert!(failures(&json!({})).is_err());
    let mut value = report();
    value["config"]["rootDir"] = json!("relative");
    assert!(failures(&value).is_err());
}

#[test]
fn flaky_playwright_expected_failures_and_skips_are_not_retried() {
    let mut value = report();
    value["stats"] = json!({"expected":1,"unexpected":0,"flaky":0,"skipped":0});
    value["suites"][0]["specs"][0]["tests"][0]["status"] = json!("expected");
    value["suites"][0]["specs"][0]["tests"][0]["expectedStatus"] = json!("failed");
    assert!(failures(&value).unwrap().is_empty());
}

fn successful_report() -> Value {
    let mut value = report();
    value["stats"]["expected"] = json!(1);
    value["stats"]["unexpected"] = json!(0);
    let test = &mut value["suites"][0]["specs"][0]["tests"][0];
    test["status"] = json!("expected");
    test["results"][0]["status"] = json!("passed");
    value
}

#[test]
fn flaky_playwright_orchestration_retries_only_listed_failures_and_keeps_recovery_on_red() {
    for (listed, pass_on, mixed, expected_calls, passes) in [
        (true, 2, false, 2, true),
        (true, 3, false, 3, true),
        (true, 4, false, 3, false),
        (false, 2, false, 1, false),
        (true, 2, true, 2, false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("gates")).unwrap();
        let entries = if listed {
            json!([{"test":"memory.shared-surfaces", "gate":"test-e2e",
            "issue":"#391", "added":"2026-10-19", "reason":"fixture"}])
        } else {
            json!([])
        };
        // Policy tests pin date handling separately; this fixture loads the current day.
        let mut entries = entries;
        if listed {
            let output = Command::new("date")
                .args(["-u", "+%Y-%m-%d"])
                .output()
                .unwrap();
            entries[0]["added"] = json!(String::from_utf8(output.stdout).unwrap().trim());
        }
        fs::write(
            directory.path().join("gates/flaky.json"),
            serde_json::to_vec(&entries).unwrap(),
        )
        .unwrap();
        let policy = flaky::Policy::load(directory.path()).unwrap();
        let path = directory.path().join("run.jsonl");
        let mut journal = flaky::Journal::create(&path).unwrap();
        let mut calls = 0;
        let result = resolve(
            Some(&policy),
            &mut journal,
            &["--reporter=json".into()],
            &directory.path().join("run.json"),
            |args, _| {
                calls += 1;
                if calls > 1 {
                    assert!(args.iter().any(|arg| arg == "--project=webgpu"));
                    assert_eq!(args.len(), 4);
                }
                if calls >= pass_on {
                    return Ok((true, successful_report()));
                }
                let mut value = report();
                if mixed && calls == 1 {
                    let mut other = value["suites"][0]["specs"][0].clone();
                    other["file"] = json!("unlisted.spec.ts");
                    value["suites"][0]["specs"]
                        .as_array_mut()
                        .unwrap()
                        .push(other);
                    value["stats"]["unexpected"] = json!(2);
                }
                Ok((false, value))
            },
        );
        assert_eq!(result.is_ok(), passes, "{result:?}");
        assert_eq!(calls, expected_calls);
        let records = fs::read_to_string(path).unwrap();
        assert_eq!(!records.is_empty(), listed && pass_on <= 3);
    }
}

#[test]
fn flaky_disabled_duel_checks_keep_the_ordinary_failure_contract_without_retrying() {
    let directory = tempfile::tempdir().unwrap();
    let mut journal = flaky::Journal::create(&directory.path().join("run.jsonl")).unwrap();
    let mut calls = 0;
    let error = resolve(
        None,
        &mut journal,
        &[],
        &directory.path().join("run.json"),
        |_, _| {
            calls += 1;
            Ok((false, report()))
        },
    )
    .unwrap_err()
    .to_string();
    assert_eq!(calls, 1);
    assert_eq!(
        error.lines().next(),
        Some("focused Playwright check unavailable: Failed(Some(1))")
    );
}

#[test]
fn flaky_playwright_keeps_nested_tests_directory_in_the_quarantine_key() {
    let mut value = report();
    value["suites"][0]["specs"][0]["file"] = json!("tests/memory.shared-surfaces.spec.ts");
    let failed = failures(&value).unwrap();
    assert_eq!(failed[0].key, "tests/memory.shared-surfaces");
    assert_eq!(
        failed[0].file,
        "/checkout/browser/tests/tests/memory.shared-surfaces.spec.ts"
    );
}
