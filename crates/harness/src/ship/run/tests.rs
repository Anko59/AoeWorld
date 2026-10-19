//! A timed-out local gate on an overloaded machine runs once more after the load falls. The
//! commands are real children that sleep past a one-second budget; the load and the wait are
//! injected, so no test waits for a real machine to calm down.
use super::{GateResult, GateVerdict, Retry, in_ci, run_gates_retrying};
use crate::{gates::registry::Gate, process::load::Load, slots::SlotEnv};
use std::{
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

const OVERLOADED: Load = Load {
    one: 30.0,
    five: 25.0,
    fifteen: 20.0,
    cores: 12,
};
const CALM: Load = Load {
    one: 1.0,
    five: 1.0,
    fifteen: 1.0,
    cores: 12,
};
/// Sleeps past the one-second budget of every test gate.
const SLEEP: &str = "sleep 3";

fn gate(command: &str) -> Gate {
    serde_json::from_value(serde_json::json!({
        "id": "slow", "command": command, "requires": [], "select": "fixture",
        "evidence": "exit status", "suites": ["static"], "cadences": ["preflight"],
        "budget_s": 1, "static": false, "heavy": false, "capabilities": [], "blocks": []
    }))
    .unwrap()
}

/// How many times the command ran: each run appends one line to `count`.
fn runs(dir: &Path) -> usize {
    fs::read_to_string(dir.join("count")).map_or(0, |text| text.lines().count())
}

/// A policy that reads `load` at every deadline, and counts the load waits it is asked for.
fn policy(local: bool, load: Load) -> (Retry, Arc<AtomicUsize>) {
    let waits = Arc::new(AtomicUsize::new(0));
    let counted = waits.clone();
    let retry = Retry::new(
        local,
        Box::new(move || Some(load)),
        Box::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }),
    );
    (retry, waits)
}

fn run_one(dir: &Path, command: &str, retry: &Retry) -> GateResult {
    let mut results = run_gates_retrying(
        dir,
        &[&gate(command)],
        &SlotEnv::default(),
        crate::slots::hold,
        retry,
    );
    assert_eq!(results.len(), 1, "one result");
    results.remove(0)
}

#[test]
fn an_overloaded_timeout_is_retried_once_and_a_pass_is_reported_with_the_retry() {
    let dir = tempfile::tempdir().unwrap();
    // The first run finds no second line in `count` and sleeps past the budget; the second passes.
    let command = format!("echo run >> count; [ \"$(wc -l < count)\" -ge 2 ] || {SLEEP}");
    let (retry, waits) = policy(true, OVERLOADED);
    let result = run_one(dir.path(), &command, &retry);
    assert_eq!(result.verdict, GateVerdict::Pass, "{}", result.line());
    assert_eq!(
        waits.load(Ordering::SeqCst),
        1,
        "the load was waited for once"
    );
    assert_eq!(
        runs(dir.path()),
        2,
        "the command ran twice, with the same budget"
    );
    let [first, second] = result.attempts.as_slice() else {
        panic!("both attempts are recorded: {}", result.line())
    };
    assert!(first.timed_out, "attempt 1 timed out: {first:?}");
    assert!(
        first.load.contains("overloaded"),
        "attempt 1 names its load: {first:?}"
    );
    assert_eq!(second.verdict, GateVerdict::Pass, "attempt 2 passed");
    assert!(!second.timed_out, "attempt 2 did not time out");
    let line = result.line();
    assert!(line.contains("attempt 1: timed out"), "{line}");
    assert!(line.contains("attempt 2: passed"), "{line}");
}

#[test]
fn an_overloaded_timeout_twice_fails_and_names_both_attempts_with_their_loads() {
    let dir = tempfile::tempdir().unwrap();
    let command = format!("echo run >> count; {SLEEP}");
    let (retry, waits) = policy(true, OVERLOADED);
    let result = run_one(dir.path(), &command, &retry);
    assert_eq!(result.verdict, GateVerdict::Fail, "{}", result.line());
    assert_eq!(
        waits.load(Ordering::SeqCst),
        1,
        "the load was waited for once"
    );
    assert_eq!(runs(dir.path()), 2, "the retry is run once, not again");
    assert!(
        result.summary.contains("over its 1s budget"),
        "the budget is not raised: {}",
        result.summary
    );
    let [first, second] = result.attempts.as_slice() else {
        panic!("both attempts are recorded: {}", result.line())
    };
    assert!(first.timed_out && second.timed_out, "{result:?}");
    assert!(first.load.contains("machine load was 30.0"), "{first:?}");
    assert!(second.load.contains("machine load was 30.0"), "{second:?}");
    let line = result.line();
    assert!(line.contains("attempt 1: timed out"), "{line}");
    assert!(line.contains("attempt 2: timed out"), "{line}");
}

#[test]
fn a_timeout_on_a_quiet_machine_is_not_retried() {
    let dir = tempfile::tempdir().unwrap();
    let command = format!("echo run >> count; {SLEEP}");
    let (retry, waits) = policy(true, CALM);
    let result = run_one(dir.path(), &command, &retry);
    assert_eq!(result.verdict, GateVerdict::Fail, "{}", result.line());
    assert_eq!(waits.load(Ordering::SeqCst), 0, "no load wait");
    assert_eq!(runs(dir.path()), 1, "the command ran once");
    assert!(result.attempts.is_empty(), "no retry is recorded");
    assert!(
        result.summary.contains("over its 1s budget"),
        "{}",
        result.summary
    );
}

#[test]
fn ci_never_retries_a_timeout_on_an_overloaded_machine() {
    let github = |name: &str| (name == "GITHUB_ACTIONS").then(|| "true".to_string());
    assert!(in_ci(github), "GITHUB_ACTIONS marks CI");
    assert!(
        in_ci(|name| (name == "CI").then(|| "1".to_string())),
        "CI marks CI"
    );
    assert!(!in_ci(|_| None), "a local run is not CI");
    assert!(
        !in_ci(|name| (name == "CI").then(String::new)),
        "an empty CI is not set"
    );

    let dir = tempfile::tempdir().unwrap();
    let command = format!("echo run >> count; {SLEEP}");
    let (retry, waits) = policy(!in_ci(github), OVERLOADED);
    let result = run_one(dir.path(), &command, &retry);
    assert_eq!(result.verdict, GateVerdict::Fail, "{}", result.line());
    assert_eq!(waits.load(Ordering::SeqCst), 0, "no load wait in CI");
    assert_eq!(runs(dir.path()), 1, "the command ran once in CI");
    assert!(result.attempts.is_empty(), "no retry is recorded in CI");
}

#[test]
fn an_ordinary_failure_on_an_overloaded_machine_is_not_retried() {
    let dir = tempfile::tempdir().unwrap();
    let (retry, waits) = policy(true, OVERLOADED);
    let result = run_one(dir.path(), "echo run >> count; exit 3", &retry);
    assert_eq!(result.verdict, GateVerdict::Fail, "{}", result.line());
    assert_eq!(waits.load(Ordering::SeqCst), 0, "no load wait");
    assert_eq!(runs(dir.path()), 1, "the command ran once");
    assert!(result.attempts.is_empty(), "no retry is recorded");
    assert!(
        !result.summary.contains("budget"),
        "not a timeout: {}",
        result.summary
    );
}

#[test]
fn a_refused_load_wait_keeps_the_first_timeout_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let retry = Retry::new(
        true,
        Box::new(|| Some(OVERLOADED)),
        Box::new(|| Err("heavy load: still 30.0 on 12 cores after 30 min".to_string())),
    );
    let command = format!("echo run >> count; {SLEEP}");
    let result = run_one(dir.path(), &command, &retry);
    assert_eq!(result.verdict, GateVerdict::Fail, "{}", result.line());
    assert_eq!(runs(dir.path()), 1, "a refused wait runs nothing again");
    assert!(
        result
            .summary
            .contains("not retried: heavy load: still 30.0"),
        "{}",
        result.summary
    );
    assert_eq!(result.attempts.len(), 1, "the first attempt is recorded");
}

#[test]
fn the_stop_check_policy_never_waits_or_retries() {
    let dir = tempfile::tempdir().unwrap();
    let command = format!("echo run >> count; {SLEEP}");
    let result = run_one(dir.path(), &command, &Retry::never());
    assert_eq!(result.verdict, GateVerdict::Fail, "{}", result.line());
    assert_eq!(runs(dir.path()), 1, "the stop check runs a gate once");
    assert!(result.attempts.is_empty(), "no retry is recorded");
}

#[test]
fn timeline_journals_overload_wait_and_both_gate_attempts() {
    let (_temp, root) = crate::ship::tests::fixture("true");
    let (retry, waits) = policy(true, OVERLOADED);
    let result = run_one(
        &root,
        &format!("echo run >> count; [ \"$(wc -l < count)\" -ge 2 ] || {SLEEP}"),
        &retry,
    );
    assert_eq!(result.verdict, GateVerdict::Pass);
    assert_eq!(waits.load(Ordering::SeqCst), 1);
    let text =
        fs::read_to_string(root.join(".git/aoe-ship/timeline/feature.jsonl")).unwrap_or_default();
    let entries: Vec<serde_json::Value> = text
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let stages: Vec<_> = entries
        .iter()
        .map(|e| (e["stage"].as_str().unwrap(), e["event"].as_str().unwrap()))
        .collect();
    assert_eq!(
        stages,
        [
            ("gate/slow", "start"),
            ("gate/slow", "end"),
            ("load-wait", "start"),
            ("load-wait", "end"),
            ("gate/slow", "start"),
            ("gate/slow", "end")
        ]
    );
    assert_eq!(entries[1]["detail"]["ok"], false);
    assert_eq!(entries[1]["detail"]["timed_out"], true);
    assert_eq!(entries[2]["detail"]["gate"], "slow");
    assert_eq!(entries[2]["detail"]["retry"], true);
    assert_eq!(entries[5]["detail"]["ok"], true);
}
