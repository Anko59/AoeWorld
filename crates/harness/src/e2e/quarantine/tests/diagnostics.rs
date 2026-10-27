use super::tests::{report, successful_report};
use super::*;
use serde_json::json;

fn error(value: Value) -> String {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("gates")).unwrap();
    fs::write(directory.path().join("gates/flaky.json"), "[]").unwrap();
    let policy = flaky::Policy::load(directory.path()).unwrap();
    let mut journal = flaky::Journal::create(&directory.path().join("run.jsonl")).unwrap();
    let mut calls = 0;
    let error = resolve(
        Some(&policy),
        &mut journal,
        &[REPORTER.into()],
        &directory.path().join("run.json"),
        |_, _| {
            calls += 1;
            Ok((false, value.clone()))
        },
    )
    .unwrap_err()
    .to_string();
    assert_eq!(calls, 1, "unlisted failures and timeouts must not retry");
    assert!(
        fs::read_to_string(directory.path().join("run.jsonl"))
            .unwrap()
            .is_empty()
    );
    error
}

#[test]
fn e2e_diagnostics_names_all_twelve_real_shard_three_failures() {
    // PR #532, run 39540701382: all 59 selected cases completed, with nine
    // actual timedOut results and three failed results (not twelve timeouts).
    let value: Value =
        serde_json::from_str(include_str!("../fixtures/ci-532-shard-3.json")).unwrap();
    let error = error(value);
    println!("{error}");
    assert!(error.starts_with("Playwright failed:"), "{error}");
    assert!(!error.contains("interrupted"), "{error}");
    for title in [
        "[canvas] five-villager presentation fixture supports box and double click selection at both viewport sizes",
        "[browser-defaults] five-villager presentation fixture supports box and double click selection at both viewport sizes",
        "[webgpu] train Militia, attack a sheep, and hunt and butcher a deer through right-click orders",
        "[canvas] dense forest resize and camera cycles keep backing and WASM memory bounded",
        "[browser-defaults] dense forest resize and camera cycles keep backing and WASM memory bounded",
        "[canvas] dense resources cover the viewport after zoom and pan",
        "[browser-defaults] dense resources cover the viewport after zoom and pan",
        "[browser-defaults] shared terrain textures, painter transitions, and selection render",
        "[browser-defaults] spectator camera fragments settle on controller-opened terrain and pan without history writes",
        "[browser-defaults] WebGL2 draws textured slopes, cliffs, skirts, water and selection pixels",
        "[browser-defaults] WebGL2 depth and material masks agree with actual software Canvas pixels",
        "[browser-defaults] WebGL2 restores textured idle-world pixels without camera or unit cache invalidation",
    ] {
        assert!(error.contains(title), "missing {title}: {error}");
    }
    assert_eq!(error.matches("status=timedOut").count(), 9);
    assert_eq!(error.matches("status=failed").count(), 3);
    assert!(error.contains("village/selection.play.spec.ts:9"));
    assert!(error.contains("duration=32795ms"));
    assert!(error.contains("Test timeout of 30000ms exceeded."));
    assert!(error.contains("expected=33 unexpected=12 skipped=14 flaky=0"));
}

#[test]
fn e2e_diagnostics_mixed_completed_results_include_titles_duration_and_first_error() {
    let mut value = report();
    let mut other = value["suites"][0]["specs"][0].clone();
    other["id"] = json!("case-b");
    other["title"] = json!("slow case");
    other["tests"][0]["annotations"][0]["description"] = json!("case-b");
    other["tests"][0]["results"][0] = json!({"status":"timedOut", "duration":1234,
        "error":{"message":"deadline exceeded\nsecond line must not appear"}});
    value["suites"][0]["specs"]
        .as_array_mut()
        .unwrap()
        .push(other);
    value["suites"][0]["specs"][0]["tests"][0]["results"][0]["error"] =
        json!({"message":format!("{}\nnot this line", "é".repeat(250))});
    let evidence = &mut value["config"]["metadata"]["aoeQuarantine"];
    evidence["selected"] = json!(["case-a", "case-b"]);
    evidence["results"]["case-b"] = json!({"errors":[], "steps":[]});
    value["stats"]["unexpected"] = json!(2);
    value["suites"][0]["title"] = json!("parent describe");
    let error = error(value);
    assert!(
        error.contains("[webgpu] parent describe memory [bounded]"),
        "{error}"
    );
    assert!(error.contains("[webgpu] parent describe slow case | status=timedOut | duration=1234ms | deadline exceeded"), "{error}");
    assert!(error.contains("é".repeat(200).as_str()));
    assert!(!error.contains(&"é".repeat(201)));
    assert!(!error.contains("second line must not appear"));
    assert!(!error.contains("not this line"));
    assert!(!error.contains("interrupted"));
    assert!(error.contains("expected=0 unexpected=2 skipped=0 flaky=0"));
}

#[test]
fn e2e_diagnostics_missing_and_interrupted_results_name_the_test() {
    for results in [json!([]), json!([{"status":"interrupted", "duration":41}])] {
        let mut value = report();
        value["suites"][0]["specs"][0]["tests"][0]["results"] = results;
        let error = error(value);
        assert!(error.starts_with("Playwright interrupted:"), "{error}");
        assert!(
            error.contains("memory.shared-surfaces.spec.ts:27 [webgpu] memory [bounded]"),
            "{error}"
        );
    }
    let mut value = successful_report();
    let evidence = &mut value["config"]["metadata"]["aoeQuarantine"];
    evidence["selected"] = json!(["case-a", "case-missing"]);
    evidence["tests"] = json!({"case-missing":{"file":"/checkout/browser/tests/absent.spec.ts", "line":12,
        "title":"describe never started", "project":"canvas"}});
    let error = error(value);
    assert!(error.starts_with("Playwright interrupted:"), "{error}");
    assert!(
        error.contains("absent.spec.ts:12 [canvas] describe never started"),
        "{error}"
    );
}

#[test]
fn e2e_diagnostics_global_errors_keep_completed_failures_visible() {
    let mut value = report();
    value["errors"] =
        json!([{"message":"worker crashed\nstack line"}, {"value":"global setup failed"}]);
    let error = error(value);
    assert!(error.starts_with("Playwright interrupted:"), "{error}");
    assert!(error.contains("global error: worker crashed"), "{error}");
    assert!(
        error.contains("global error: global setup failed"),
        "{error}"
    );
    assert!(error.contains("memory.shared-surfaces.spec.ts:27 [webgpu] memory [bounded]"));
    assert!(error.contains("status=failed"));
    assert!(!error.contains("stack line"));
}

#[test]
fn e2e_diagnostics_caps_failure_entries_and_reports_the_remainder() {
    let mut value = report();
    let spec = value["suites"][0]["specs"][0].clone();
    let mut specs = Vec::new();
    let mut selected = Vec::new();
    let raw = value["config"]["metadata"]["aoeQuarantine"]["results"]["case-a"].clone();
    for i in 0..45 {
        let id = format!("case-{i}");
        let mut spec = spec.clone();
        spec["id"] = json!(id);
        spec["title"] = json!(format!("failure {i}"));
        spec["tests"][0]["annotations"][0]["description"] = json!(id);
        selected.push(id.clone());
        value["config"]["metadata"]["aoeQuarantine"]["results"][&id] = raw.clone();
        specs.push(spec);
    }
    value["suites"][0]["specs"] = json!(specs);
    value["stats"]["unexpected"] = json!(45);
    value["config"]["metadata"]["aoeQuarantine"]["selected"] = json!(selected);
    let error = error(value);
    assert_eq!(error.matches("status=failed").count(), 40, "{error}");
    assert!(error.contains("5 more"), "{error}");
    assert!(error.contains("unexpected=45"));
}

#[test]
fn e2e_diagnostics_recovers_listed_assertion_alongside_unrecoverable_listed_timeout() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("gates")).unwrap();
    let date = Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .unwrap();
    fs::write(
        directory.path().join("gates/flaky.json"),
        serde_json::to_vec(&json!([{
            "test":"memory.shared-surfaces", "gate":"test-e2e", "issue":"#391",
            "added":String::from_utf8(date.stdout).unwrap().trim(), "reason":"regression"
        }]))
        .unwrap(),
    )
    .unwrap();
    let policy = flaky::Policy::load(directory.path()).unwrap();
    let journal_path = directory.path().join("run.jsonl");
    let mut journal = flaky::Journal::create(&journal_path).unwrap();
    let mut value = report();
    let mut timeout = value["suites"][0]["specs"][0].clone();
    timeout["id"] = json!("case-b");
    timeout["title"] = json!("listed timeout");
    timeout["tests"][0]["annotations"][0]["description"] = json!("case-b");
    timeout["tests"][0]["results"][0] = json!({"status":"timedOut", "duration":1234,
        "error":{"message":"test deadline"}});
    value["suites"][0]["specs"]
        .as_array_mut()
        .unwrap()
        .push(timeout);
    value["stats"]["unexpected"] = json!(2);
    let evidence = &mut value["config"]["metadata"]["aoeQuarantine"];
    evidence["selected"] = json!(["case-a", "case-b"]);
    evidence["results"]["case-b"] = json!({"errors":[{"message":"test deadline"}],
        "steps":[{"category":"hook", "error":{"message":"context.close timed out"}, "steps":[]}]});
    let mut calls = 0;
    let error = resolve(
        Some(&policy),
        &mut journal,
        &[REPORTER.into()],
        &directory.path().join("run.json"),
        |_, _| {
            calls += 1;
            if calls == 1 {
                Ok((false, value.clone()))
            } else {
                Ok((true, successful_report()))
            }
        },
    )
    .unwrap_err()
    .to_string();
    assert_eq!(calls, 2, "only the assertion gets a retry");
    assert!(
        error.contains("[webgpu] listed timeout | status=timedOut"),
        "{error}"
    );
    assert!(
        !error.contains("[webgpu] memory [bounded]"),
        "recovered assertion: {error}"
    );
    assert!(!error.contains("interrupted"));
    assert_eq!(fs::read_to_string(journal_path).unwrap().lines().count(), 1);
}

#[test]
fn e2e_diagnostics_retry_interruption_keeps_other_initial_failures_visible() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("gates")).unwrap();
    let date = Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .unwrap();
    fs::write(
        directory.path().join("gates/flaky.json"),
        serde_json::to_vec(&json!([{
            "test":"memory.shared-surfaces", "gate":"test-e2e", "issue":"#391",
            "added":String::from_utf8(date.stdout).unwrap().trim(), "reason":"regression"
        }]))
        .unwrap(),
    )
    .unwrap();
    let policy = flaky::Policy::load(directory.path()).unwrap();
    let mut journal = flaky::Journal::create(&directory.path().join("run.jsonl")).unwrap();
    let mut initial = report();
    let mut other = initial["suites"][0]["specs"][0].clone();
    other["id"] = json!("case-b");
    other["title"] = json!("unlisted initial failure");
    other["file"] = json!("unlisted.spec.ts");
    other["tests"][0]["annotations"][0]["description"] = json!("case-b");
    initial["suites"][0]["specs"]
        .as_array_mut()
        .unwrap()
        .push(other);
    initial["stats"]["unexpected"] = json!(2);
    let evidence = &mut initial["config"]["metadata"]["aoeQuarantine"];
    evidence["selected"] = json!(["case-a", "case-b"]);
    evidence["results"]["case-b"] = evidence["results"]["case-a"].clone();
    let mut retry = report();
    retry["suites"][0]["specs"][0]["tests"][0]["results"][0]["status"] = json!("interrupted");
    let mut calls = 0;
    let error = resolve(
        Some(&policy),
        &mut journal,
        &[REPORTER.into()],
        &directory.path().join("run.json"),
        |_, _| {
            calls += 1;
            Ok((
                false,
                if calls == 1 {
                    initial.clone()
                } else {
                    retry.clone()
                },
            ))
        },
    )
    .unwrap_err()
    .to_string();
    assert_eq!(calls, 2);
    assert!(error.contains("Playwright interrupted:"), "{error}");
    assert!(
        error.contains(
            "memory.shared-surfaces.spec.ts:27 [webgpu] memory [bounded] | status=interrupted"
        ),
        "{error}"
    );
    assert!(
        error.contains("unlisted.spec.ts:27 [webgpu] unlisted initial failure | status=failed"),
        "{error}"
    );
    assert!(
        error.contains("expected=0 unexpected=2 skipped=0 flaky=0"),
        "{error}"
    );
}
