use super::super::{duel::setup, fixture, showcase_security::ENV_LOCK};
use crate::ship::{duel::run as duel, git};
use std::{fs, path::Path};

/// From the second implementer launch on, each launch writes a new non-test file (or `bad` code).
fn fixing_implementer(root: &Path, later: &str) {
    let runtime = root.join(".cache/bin/codex");
    let script = fs::read_to_string(&runtime).unwrap().replace(
        "implementer:*) echo green > crates/x/src/lib.rs",
        &format!("implementer:*) echo green > crates/x/src/lib.rs; n=$(grep -c implementer .cache/calls); if test \"$n\" -ge 2; then {later}; fi"),
    );
    fs::write(runtime, script).unwrap();
}

fn commits(root: &Path, phase: &str) -> usize {
    git::git(root, &["log", "--format=%B"])
        .unwrap()
        .matches(&format!("Harness-Phase: {phase}"))
        .count()
}

#[test]
fn qualify_pass_is_reported_and_tells_the_main_session_what_is_left() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, mut options) = setup(&root, "happy");
    options.rounds = 1;
    options.qualify = Some("echo qualifying".into());
    let summary = duel(&root, "Task", &options).unwrap();
    assert!(
        summary.contains("qualify: PASS (echo qualifying, 0s)"),
        "{summary}"
    );
    assert!(
        summary.contains("Result: PASS, qualified")
            && summary.contains("read the diff")
            && summary.contains("film")
            && summary.contains("make ship"),
        "{summary}"
    );
    assert_eq!(
        fs::read_to_string(root.join(".cache/calls")).unwrap(),
        "tester\nimplementer\n"
    );
    assert_eq!(commits(&root, "qualification"), 0);
}

#[test]
fn qualify_failure_is_fixed_by_the_implementer_in_its_own_commit_and_rerun() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, mut options) = setup(&root, "happy");
    options.rounds = 1;
    options.qualify =
        Some("test -f crates/x/src/q.txt || { echo lint-failure-line; exit 1; }".into());
    fixing_implementer(&root, "echo ok > crates/x/src/q.txt");
    let summary = duel(&root, "Task", &options).unwrap();
    assert!(summary.contains("qualify: FAIL"), "{summary}");
    assert!(summary.contains("lint-failure-line"), "{summary}");
    assert!(summary.contains("qualify: PASS ("), "{summary}");
    assert_eq!(
        fs::read_to_string(root.join(".cache/calls")).unwrap(),
        "tester\nimplementer\nimplementer\n"
    );
    let prompts = fs::read_to_string(root.join(".cache/prompts")).unwrap();
    assert!(
        prompts.contains("qualification command") && prompts.contains("WITHOUT touching any test"),
        "{prompts}"
    );
    let last = git::git(&root, &["log", "-1", "--format=%B"]).unwrap();
    assert!(last.contains("Harness-Phase: qualification"), "{last}");
    assert!(last.contains("Co-Authored-By:"), "{last}");
    assert_eq!(commits(&root, "tests"), 1);
    assert_eq!(commits(&root, "implementation"), 1);
    assert!(git::subject(&root).is_ok());
}

#[test]
fn qualify_failing_twice_ends_fail_and_keeps_every_earlier_commit() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, mut options) = setup(&root, "happy");
    options.rounds = 1;
    options.qualify = Some("echo still-broken; exit 1".into());
    fixing_implementer(&root, "echo $n > crates/x/src/q.txt");
    let error = duel(&root, "Task", &options).unwrap_err();
    assert!(
        error.starts_with("qualification failed:") && error.contains("still-broken"),
        "{error}"
    );
    assert_eq!(
        fs::read_to_string(root.join(".cache/calls")).unwrap(),
        "tester\nimplementer\nimplementer\nimplementer\n"
    );
    assert_eq!(commits(&root, "tests"), 1);
    assert_eq!(commits(&root, "implementation"), 1);
    assert_eq!(commits(&root, "qualification"), 2);
    let summary = fs::read_to_string(root.join(".cache/duel/feature.md")).unwrap();
    assert!(
        summary.contains("Result: FAIL: qualification failed:"),
        "{summary}"
    );
    assert!(git::subject(&root).is_ok());
}

#[test]
fn a_test_touched_during_a_qualification_fix_is_refused_and_restored() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, mut options) = setup(&root, "happy");
    options.rounds = 1;
    options.qualify = Some("exit 1".into());
    fixing_implementer(&root, "echo weakened >> crates/x/src/tests.rs");
    let error = duel(&root, "Task", &options).unwrap_err();
    assert!(
        error.contains("implementer changed forbidden paths") && error.contains("tests.rs"),
        "{error}"
    );
    assert!(
        !fs::read_to_string(root.join("crates/x/src/tests.rs"))
            .unwrap()
            .contains("weakened")
    );
    assert_eq!(commits(&root, "qualification"), 0);
    assert!(git::subject(&root).is_ok());
}

#[test]
fn an_implementer_that_cannot_fix_a_test_failure_says_so_and_stops() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, mut options) = setup(&root, "happy");
    options.rounds = 1;
    options.qualify = Some("exit 1".into());
    fixing_implementer(
        &root,
        "echo 'DUEL_QUALIFY_BLOCKED: the failing test is protected'",
    );
    let error = duel(&root, "Task", &options).unwrap_err();
    assert!(
        error.starts_with("qualification blocked: the failing test is protected"),
        "{error}"
    );
    assert_eq!(commits(&root, "qualification"), 0);
}

#[test]
fn without_qualify_the_duel_is_unchanged() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, mut options) = setup(&root, "happy");
    options.rounds = 1;
    let summary = duel(&root, "Task", &options).unwrap();
    assert!(
        !summary.contains("Qualification") && !summary.contains("qualif"),
        "{summary}"
    );
    assert!(summary.contains("Result: PASS. Main session"), "{summary}");
    assert_eq!(
        fs::read_to_string(root.join(".cache/calls")).unwrap(),
        "tester\nimplementer\n"
    );
}

#[test]
fn a_default_check_green_line_shows_a_count_not_test_names() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, mut options) = setup(&root, "happy");
    options.rounds = 1;
    let summary = duel(&root, "Task", &options).unwrap();
    assert!(
        summary.contains("green: PASS (attempt 1); tests: 1 passed"),
        "{summary}"
    );
}

#[test]
fn an_override_summary_names_failing_tests_and_lists_no_passing_ones() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, mut options) = setup(&root, "happy");
    options.rounds = 1;
    fs::write(root.join(".cache/checks"), "fixture\n").unwrap();
    let passing: String = (0..60)
        .map(|n| format!("printf 'PASS [ 0.1s] aoe-x::mod passing_case_{n}\\n'\n"))
        .collect();
    fs::write(
        root.join(".cache/check.sh"),
        format!("{passing}if grep -q green crates/x/src/lib.rs; then exit 0; fi\nprintf 'FAIL [ 0.1s] aoe-x::mod broken_case\\n'\nexit 1\n"),
    )
    .unwrap();
    options.check = Some("sh .cache/check.sh".into());
    let summary = duel(&root, "Task", &options).unwrap();
    assert!(
        summary.contains("red: FAIL; tests: aoe-x::mod broken_case"),
        "{summary}"
    );
    assert!(
        summary.contains("green: PASS (attempt 1); tests: 60 passed"),
        "{summary}"
    );
    assert!(!summary.contains("passing_case_"), "{summary}");
    assert!(!summary.contains("unidentified"), "{summary}");
}
