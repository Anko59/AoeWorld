use super::*;
use serde_json::json;

fn entry() -> serde_json::Value {
    json!({"test":"tiling_pixels::flaky", "gate":"test-wasm", "issue":"#391",
        "added":"2026-10-19", "reason":"Intermittent context loss"})
}

fn policy() -> Policy {
    Policy::parse_at(
        &serde_json::to_vec(&json!([entry()])).unwrap(),
        date("2026-10-19").unwrap(),
    )
    .unwrap()
}

#[test]
fn flaky_schema_is_strict_and_requires_tracking_metadata() {
    let today = date("2026-10-19").unwrap();
    for field in ["test", "gate", "issue", "added", "reason"] {
        let mut value = entry();
        value.as_object_mut().unwrap().remove(field);
        assert!(
            Policy::parse_at(&serde_json::to_vec(&json!([value])).unwrap(), today).is_err(),
            "{field}"
        );
    }
    for (field, invalid) in [
        ("extra", "x"),
        ("gate", "test-unit"),
        ("test", ""),
        ("issue", "391"),
        ("issue", "#0"),
        ("reason", "  "),
        ("added", "2026-02-30"),
        ("added", "2026-1-19"),
        ("added", "2026-10-20"),
    ] {
        let mut value = entry();
        value[field] = json!(invalid);
        assert!(
            Policy::parse_at(&serde_json::to_vec(&json!([value])).unwrap(), today).is_err(),
            "{field}={invalid}"
        );
    }
    assert!(
        Policy::parse_at(
            &serde_json::to_vec(&json!([entry(), entry()])).unwrap(),
            today
        )
        .is_err()
    );
    assert!(Policy::parse_at(b"{}", today).is_err());
}

#[test]
fn flaky_expiry_allows_fourteen_days_but_fails_on_fifteen() {
    let bytes = serde_json::to_vec(&json!([entry()])).unwrap();
    assert!(Policy::parse_at(&bytes, date("2026-11-02").unwrap()).is_ok());
    let error = Policy::parse_at(&bytes, date("2026-11-03").unwrap())
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("expired") && error.contains("#391"),
        "{error}"
    );
    assert_eq!(date("2024-03-01").unwrap() - date("2024-02-28").unwrap(), 2);
    assert!(date("2025-02-29").is_err());
    assert_eq!(date("1970-01-01").unwrap(), 0);
}

#[test]
fn flaky_fail_then_pass_is_reported_and_stops_retrying() {
    let policy = policy();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("run.jsonl");
    let mut journal = Journal::create(&path).unwrap();
    let mut calls = 0;
    assert!(
        retry(
            policy.find("test-wasm", "tiling_pixels::flaky"),
            "tiling_pixels::flaky",
            &mut journal,
            &mut 0,
            || {
                calls += 1;
                Ok(true)
            }
        )
        .unwrap()
    );
    assert_eq!(calls, 1);
    let lines = fs::read_to_string(&path).unwrap();
    let records: Vec<serde_json::Value> = lines
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        records,
        [json!({"test":"tiling_pixels::flaky", "gate":"test-wasm", "issue":"#391", "attempt":2})]
    );
    assert_eq!(
        message("tiling_pixels::flaky", 2, "#391"),
        "flaky: tiling_pixels::flaky passed on attempt 2 (quarantined, #391)"
    );
}

#[test]
fn flaky_fail_fail_pass_records_attempt_three() {
    let policy = policy();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("run.jsonl");
    let mut journal = Journal::create(&path).unwrap();
    let mut calls = 0;
    assert!(
        retry(
            policy.find("test-wasm", "tiling_pixels::flaky"),
            "tiling_pixels::flaky",
            &mut journal,
            &mut 0,
            || {
                calls += 1;
                Ok(calls == 2)
            }
        )
        .unwrap()
    );
    assert_eq!(calls, 2);
    assert!(fs::read_to_string(path).unwrap().contains("\"attempt\":3"));
}

#[test]
fn flaky_fail_fail_fail_and_unlisted_failure_stay_red() {
    let policy = policy();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("run.jsonl");
    let mut journal = Journal::create(&path).unwrap();
    let mut calls = 0;
    assert!(
        !retry(
            policy.find("test-wasm", "tiling_pixels::flaky"),
            "tiling_pixels::flaky",
            &mut journal,
            &mut 0,
            || {
                calls += 1;
                Ok(false)
            }
        )
        .unwrap()
    );
    assert_eq!(calls, 2);
    for (gate, test) in [
        ("test-wasm", "unlisted"),
        ("test-e2e", "tiling_pixels::flaky"),
    ] {
        assert!(
            !retry(
                policy.find(gate, test),
                test,
                &mut journal,
                &mut 0,
                || panic!("unlisted test retried")
            )
            .unwrap()
        );
    }
    assert!(fs::read_to_string(path).unwrap().is_empty());
}

#[test]
fn flaky_retry_infrastructure_errors_are_not_retried_or_hidden() {
    let policy = policy();
    let directory = tempfile::tempdir().unwrap();
    let mut journal = Journal::create(&directory.path().join("run.jsonl")).unwrap();
    let mut calls = 0;
    assert!(
        retry(
            policy.find("test-wasm", "tiling_pixels::flaky"),
            "flaky",
            &mut journal,
            &mut 0,
            || {
                calls += 1;
                Err("cancelled".into())
            }
        )
        .is_err()
    );
    assert_eq!(calls, 1);
}

#[test]
fn flaky_docs_gate_rejects_expired_entries_before_checking_docs() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("gates")).unwrap();
    let mut value = entry();
    value["added"] = json!("1970-01-01");
    fs::write(
        directory.path().join("gates/flaky.json"),
        serde_json::to_vec(&json!([value])).unwrap(),
    )
    .unwrap();
    let error = crate::gates::docs_check(directory.path())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("expired") && error.contains("#391"),
        "{error}"
    );
}

#[test]
fn flaky_journal_is_fresh_for_each_run() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("run.jsonl");
    fs::write(&path, "stale pass\n").unwrap();
    let _journal = Journal::create(&path).unwrap();
    assert!(fs::read_to_string(path).unwrap().is_empty());
}
