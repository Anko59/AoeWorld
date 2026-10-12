use super::{
    duel::{Env, setup},
    fixture,
    showcase_security::ENV_LOCK,
};
use crate::ship::{duel::run as duel, git};
use std::{fs, time::Duration};

#[test]
fn a_failing_attack_runs_a_second_red_green_pair() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, options) = setup(&root, "attack");
    let base = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let summary = duel(&root, "Task", &options).unwrap();
    assert_eq!(summary.matches("Tests commit:").count(), 2);
    assert_eq!(summary.matches("Implementation commit:").count(), 2);
    assert_eq!(
        git::git(&root, &["rev-list", "--count", &format!("{base}..HEAD")]).unwrap(),
        "4"
    );
}

#[test]
fn default_browser_check_override_and_unusual_test_paths() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for mode in ["browser", "space"] {
        let (_temp, root) = fixture("true");
        let (_env, mut options) = setup(&root, mode);
        options.rounds = 1;
        let summary = duel(&root, "Task", &options).unwrap();
        assert!(summary.contains(if mode == "browser" {
            "make test-e2e"
        } else {
            "make test-unit"
        }));
        assert!(git::subject(&root).is_ok());
    }
}

#[test]
fn failed_implementer_keeps_tests_and_work_and_incomplete_check_cannot_be_red() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for mode in ["exit-impl", "happy"] {
        let (_temp, root) = fixture("true");
        let (_env, mut options) = setup(&root, mode);
        if mode == "happy" {
            options.check = Some("sleep 30".into());
            options.budget = Duration::from_millis(100);
        }
        let error = duel(&root, "Task", &options).unwrap_err();
        if mode == "happy" {
            assert!(error.contains("check could not complete"));
            assert!(git::subject(&root).is_ok());
        } else {
            assert!(error.contains("implementer did not finish"));
            assert!(
                git::git(&root, &["log", "-1", "--format=%B"])
                    .unwrap()
                    .contains("Harness-Phase: tests")
            );
            assert_eq!(
                fs::read_to_string(root.join("crates/x/src/lib.rs")).unwrap(),
                "pending\n"
            );
        }
    }
}

#[test]
fn main_only_entry_reads_task_and_environment() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, _) = setup(&root, "happy");
    let _config = Env::set(&[
        ("DUEL_TASK", ".cache/task.md".into()),
        ("DUEL_RUNTIME", "codex".into()),
        ("AOE_AGENT_ROLE", "tester".into()),
    ]);
    fs::write(root.join(".cache/task.md"), "Task").unwrap();
    assert!(
        crate::ship::duel::execute(&root)
            .unwrap_err()
            .contains("main session")
    );
    let _main = Env::set(&[("AOE_AGENT_ROLE", "main".into())]);
    crate::ship::duel::execute(&root).unwrap();
    let _missing = Env::set(&[("DUEL_TASK", "".into())]);
    assert!(
        crate::ship::duel::execute(&root)
            .unwrap_err()
            .contains("DUEL_TASK is required")
    );
}

#[test]
fn undecodable_paths_are_refused_and_restored() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, options) = setup(&root, "nonutf8");
    let error = duel(&root, "Task", &options).unwrap_err();
    assert!(
        error.contains("UTF-8") && error.contains("restored"),
        "{error}"
    );
    assert!(git::subject(&root).is_ok());
}
