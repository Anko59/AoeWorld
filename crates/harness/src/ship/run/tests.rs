//! Refusals and overloaded deadlines stop immediately; only suspend cleanup retries.
use super::{GateResult, GateVerdict, Retry, in_ci, run_gates_retrying};
use crate::{gates::registry::Gate, jobs::JobEnv, process::load::Load};
use std::{fs, path::Path};

mod suspend;
const OVERLOADED: Load = Load {
    one: 30.0,
    five: 25.0,
    fifteen: 20.0,
    cores: 12,
};

fn gate(command: &str) -> Gate {
    serde_json::from_value(serde_json::json!({
        "id":"slow", "command":command, "requires":[], "select":"fixture",
        "evidence":"exit status", "suites":["static"], "cadences":["preflight"],
        "budget_s":1, "static":false, "long":false, "capabilities":[], "blocks":[]
    }))
    .unwrap()
}
fn runs(dir: &Path) -> usize {
    fs::read_to_string(dir.join("count")).map_or(0, |text| text.lines().count())
}
fn policy() -> Retry {
    Retry {
        load: Box::new(|| Some(OVERLOADED)),
    }
}
fn run_one(dir: &Path, command: &str, retry: &Retry) -> GateResult {
    run_gates_retrying(
        dir,
        &[&gate(command)],
        &JobEnv::default(),
        crate::jobs::hold,
        retry,
    )
    .remove(0)
}

#[test]
fn overloaded_deadlines_fail_once_without_a_load_wait_or_budget_change() {
    let dir = tempfile::tempdir().unwrap();
    let result = run_one(dir.path(), "echo run >> count; sleep 3", &policy());
    assert_eq!(result.verdict, GateVerdict::Fail);
    assert_eq!(runs(dir.path()), 1);
    assert!(result.attempts.is_empty());
    assert!(
        result.summary.contains("over its 1s budget"),
        "{}",
        result.summary
    );
    assert!(result.summary.contains("overloaded"), "{}", result.summary);
}

#[test]
fn admission_refusal_stops_the_gate_sequence_without_starting_any_command() {
    fn busy(_: &Path, _: &str, _: &JobEnv) -> crate::jobs::Result<crate::jobs::guard::Guard> {
        Err(Box::new(crate::jobs::lock::Busy(
            "make coverage on other in /work/other (started now)".into(),
        )))
    }
    let dir = tempfile::tempdir().unwrap();
    let first = gate("echo run >> count");
    let mut second = gate("echo run >> count");
    second.id = "second".into();
    let started = std::time::Instant::now();
    let results = run_gates_retrying(
        dir.path(),
        &[&first, &second],
        &JobEnv::default(),
        busy,
        &policy(),
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    assert_eq!(
        results.len(),
        1,
        "refusal stops and reports without further work"
    );
    assert_eq!(results[0].verdict, GateVerdict::Unavailable);
    assert!(results[0].summary.contains("busy: make coverage"));
    assert_eq!(runs(dir.path()), 0);
}

#[test]
fn ci_detection_uses_nonempty_ci_markers() {
    assert!(in_ci(
        |name| (name == "GITHUB_ACTIONS").then(|| "true".into())
    ));
    assert!(in_ci(|name| (name == "CI").then(|| "1".into())));
    assert!(!in_ci(|_| None));
    assert!(!in_ci(|name| (name == "CI").then(String::new)));
}

#[test]
fn timeline_reports_one_gate_attempt_and_no_load_wait() {
    let (_temp, root) = crate::ship::tests::fixture("true");
    let result = run_one(&root, "echo run >> count; sleep 3", &policy());
    assert_eq!(result.verdict, GateVerdict::Fail);
    let text = fs::read_to_string(root.join(".git/aoe-ship/timeline/feature.jsonl")).unwrap();
    let entries: Vec<serde_json::Value> = text
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| e["stage"] == "gate/slow"));
    assert_eq!(entries[1]["detail"]["timed_out"], true);
}
