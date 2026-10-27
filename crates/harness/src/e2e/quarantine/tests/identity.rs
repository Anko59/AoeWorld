use super::tests::{report, successful_report};
use super::*;
use serde_json::json;

fn policy(directory: &Path) -> flaky::Policy {
    fs::create_dir(directory.join("gates")).unwrap();
    let date = Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .unwrap();
    fs::write(
        directory.join("gates/flaky.json"),
        serde_json::to_vec(&json!([{
            "test":"memory.shared-surfaces", "gate":"test-e2e", "issue":"#391",
            "added":String::from_utf8(date.stdout).unwrap().trim(), "reason":"identity regression"
        }]))
        .unwrap(),
    )
    .unwrap();
    flaky::Policy::load(directory).unwrap()
}

fn camera(mut value: Value) -> Value {
    value["suites"][0]["specs"][0]["title"] = json!("camera reset");
    value["suites"][0]["specs"][0]["column"] = json!(1);
    value
}

fn run(initial: Value, retry: Value) -> (Result<()>, String, usize) {
    let directory = tempfile::tempdir().unwrap();
    let policy = policy(directory.path());
    let journal_path = directory.path().join("run.jsonl");
    let mut journal = flaky::Journal::create(&journal_path).unwrap();
    let mut calls = 0;
    let result = resolve(
        Some(&policy),
        &mut journal,
        &[REPORTER.into()],
        &directory.path().join("run.json"),
        |_, _| {
            calls += 1;
            assert!(calls <= 2, "only the assertion may retry");
            Ok(if calls == 1 {
                (false, initial.clone())
            } else {
                (true, retry.clone())
            })
        },
    );
    (result, fs::read_to_string(journal_path).unwrap(), calls)
}

fn with_timeout(kind: &str) -> Value {
    let mut value = camera(report());
    let mut other = value["suites"][0]["specs"][0].clone();
    other["id"] = json!("case-b");
    other["tests"][0]["annotations"][0]["description"] = json!("case-b");
    other["tests"][0]["results"][0] = json!({"status":"timedOut", "duration":1234,
        "error":{"message":"test deadline"}});
    match kind {
        "title path" => {
            other["title"] = json!("reset");
            other["line"] = json!(31);
            value["suites"][0]["suites"] = json!([{"title":"camera", "specs":[other]}]);
        }
        "line" | "column" | "project" => {
            match kind {
                "line" => other["line"] = json!(31),
                "column" => other["column"] = json!(20),
                "project" => other["tests"][0]["projectName"] = json!("canvas"),
                _ => unreachable!(),
            }
            value["suites"][0]["specs"]
                .as_array_mut()
                .unwrap()
                .push(other);
        }
        _ => unreachable!(),
    }
    value["stats"]["unexpected"] = json!(2);
    let evidence = &mut value["config"]["metadata"]["aoeQuarantine"];
    evidence["selected"] = json!(["case-a", "case-b"]);
    evidence["results"]["case-b"] = json!({"errors":[], "steps":[]});
    value
}

#[test]
fn quarantine_identity_top_level_recovery_keeps_nested_camera_timeout_failed() {
    // Exact review trigger: test("camera reset") recovers, while
    // describe("camera") > test("reset") at a different line times out.
    assert_timeout_remains("title path");
}

#[test]
fn quarantine_identity_same_title_file_different_lines_remains_failed() {
    assert_timeout_remains("line");
}

#[test]
fn quarantine_identity_same_title_file_different_columns_remains_failed() {
    assert_timeout_remains("column");
}

#[test]
fn quarantine_identity_same_title_different_projects_remains_failed() {
    assert_timeout_remains("project");
}

fn assert_timeout_remains(kind: &str) {
    let (result, journal, calls) = run(with_timeout(kind), camera(successful_report()));
    let error = result
        .expect_err("distinct timeout must keep the gate failed")
        .to_string();
    println!("{kind}: {error}");
    assert_eq!(calls, 2);
    assert_eq!(journal.lines().count(), 1, "only the assertion recovered");
    assert!(error.starts_with("Playwright failed:"), "{error}");
    assert_eq!(error.matches("status=timedOut").count(), 1, "{error}");
    assert!(
        !error.contains("status=failed"),
        "recovered case leaked: {error}"
    );
    let expected = if kind == "project" {
        "[canvas] camera reset"
    } else {
        "[webgpu] camera reset"
    };
    assert!(error.contains(expected), "{error}");
}

#[test]
fn quarantine_identity_rejects_a_retry_of_any_other_full_identity() {
    for field in ["file", "line", "column", "project", "title path"] {
        let initial = camera(report());
        let mut retry = camera(successful_report());
        let spec = &mut retry["suites"][0]["specs"][0];
        match field {
            "file" => spec["file"] = json!("other.spec.ts"),
            "line" => spec["line"] = json!(28),
            "column" => spec["column"] = json!(20),
            "project" => spec["tests"][0]["projectName"] = json!("canvas"),
            "title path" => {
                spec["title"] = json!("reset");
                retry["suites"][0]["title"] = json!("camera");
            }
            _ => unreachable!(),
        }
        let (result, journal, calls) = run(initial, retry);
        let error = result.expect_err(field).to_string();
        assert!(
            error.contains("rerun selected a different case"),
            "{field}: {error}"
        );
        assert_eq!(calls, 2);
        assert!(journal.is_empty(), "wrong {field} earned recovery");
    }
}
