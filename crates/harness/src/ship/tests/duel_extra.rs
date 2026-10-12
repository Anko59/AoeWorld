use super::{
    duel::{Env, setup},
    fixture, run,
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
        assert!(summary.contains("make duel-check HARNESS_DUEL_BASE="));
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

#[test]
fn slash_branch_summary_has_full_task_names_commits_and_reports() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, mut options) = setup(&root, "happy");
    options.check = Some("make test-unit".into());
    run(&root, &["checkout", "-qb", "scratch/duel-trial"]);
    let task = "Task title\nDetailed second line  \nDetailed third line";
    let summary = duel(&root, task, &options).unwrap();
    let path = root.join(".cache/duel/scratch_duel-trial.md");
    assert_eq!(fs::read_to_string(path).unwrap(), summary);
    assert!(summary.contains(task));
    assert!(summary.contains("Codex / stub-model"));
    assert!(summary.contains("red: FAIL") && summary.contains("green: PASS"));
    assert!(summary.contains("tests: task_test"));
    assert!(summary.contains("Tests commit:") && summary.contains("Implementation commit:"));
    assert!(summary.contains("tester report") && summary.contains("implementer report"));
    let messages = git::git(&root, &["log", "-2", "--format=%B"]).unwrap();
    assert_eq!(messages.matches(task).count(), 2);
}

#[test]
fn pre_existing_green_failures_stop_retries_only_when_untouched_and_reproduced() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for mode in [
        "pre-existing",
        "new-failure",
        "changed-test",
        "build-failure",
    ] {
        let (_temp, root) = fixture("true");
        let (_env, mut options) = setup(&root, mode);
        fs::write(root.join(".cache/checks"), "fixture\n").unwrap();
        options.rounds = 1;
        let make = format!(
            r#"export HARNESS_DUEL_REVISION
duel-check:
	@echo checked >> .cache/checks
	@mkdir -p .cache/duel
	@if test -n "$$HARNESS_DUEL_REVISION"; then \
	  echo '{{"passed":false,"tests":["existing"],"failed":["{}"],"untouched":[]}}' > .cache/duel/check.json; \
	elif grep -q green crates/x/src/lib.rs; then \
	  echo '{{"passed":false,"tests":["existing"],"failed":["existing"],"untouched":[{}]}}' > .cache/duel/check.json; \
	else \
	  echo '{{"passed":false,"tests":["task_test"],"failed":["task_test"],"untouched":[]}}' > .cache/duel/check.json; \
	fi
	@exit 1
"#,
            if mode == "new-failure" {
                "different"
            } else {
                "existing"
            },
            if mode == "changed-test" || mode == "build-failure" {
                ""
            } else {
                "\"existing\""
            }
        );
        fs::write(root.join("Makefile"), make).unwrap();
        run(&root, &["add", "Makefile"]);
        run(&root, &["commit", "-qm", "named check stub"]);
        let result = duel(&root, "Task\nDetails", &options);
        let calls = fs::read_to_string(root.join(".cache/calls")).unwrap();
        if mode == "pre-existing" {
            let summary = result.unwrap();
            assert!(summary.contains("pre-existing failure: existing"));
            assert!(summary.contains("green: FAIL (attempt 1); tests: existing"));
            assert_eq!(calls.matches("implementer").count(), 1);
            assert!(summary.contains("Implementation commit:"));
            assert!(git::subject(&root).is_ok());
        } else {
            assert!(result.unwrap_err().contains("3 attempts"));
            assert_eq!(calls.matches("implementer").count(), 3);
        }
        let prompts = fs::read_to_string(root.join(".cache/prompts")).unwrap();
        assert!(prompts.contains("Focused check: `make duel-check HARNESS_DUEL_BASE="));
        assert!(prompts.contains("Do not run make preflight or the whole suite"));
    }
}

#[test]
fn focused_check_executes_selected_packages_reports_names_and_rejects_build_failure() {
    use crate::ship::duel::focus;
    use std::os::unix::fs::PermissionsExt;
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, _) = setup(&root, "happy");
    fs::write(root.join(".cache/checks"), "fixture\n").unwrap();
    fs::write(
        root.join("crates/x/Cargo.toml"),
        "[package]\nname='aoe-x'\n",
    )
    .unwrap();
    run(&root, &["add", "crates/x/Cargo.toml"]);
    run(&root, &["commit", "-qm", "package"]);
    let base = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    fs::write(
        root.join("crates/x/src/tests.rs"),
        "#[test]\nfn task_test() {}\n",
    )
    .unwrap();
    let cargo = root.join(".cache/bin/cargo");
    fs::write(
        &cargo,
        r#"#!/bin/sh
printf '%s\n' "$*" >> .cache/cargo-calls
if test "$DUEL_STUB" = build; then echo compiler-error >&2; exit 101; fi
printf '{"type":"test","event":"%s","name":"aoe-x::lib$task_test"}\n' "$DUEL_STUB"
if test "$DUEL_STUB" = failed; then exit 100; fi
"#,
    )
    .unwrap();
    fs::set_permissions(cargo, fs::Permissions::from_mode(0o700)).unwrap();
    let _base = Env::set(&[
        ("HARNESS_DUEL_BASE", base),
        ("HARNESS_DUEL_REVISION", "".into()),
    ]);
    for mode in ["failed", "ok", "build"] {
        let _mode = Env::set(&[("DUEL_STUB", mode.into())]);
        let result = focus::execute(&root);
        let finished = crate::ship::duel::stages::browser(&root);
        assert_eq!(finished.is_ok(), mode == "ok");
        let report: focus::CheckReport =
            serde_json::from_slice(&fs::read(root.join(".cache/duel/check.json")).unwrap())
                .unwrap();
        if mode == "ok" {
            assert!(result.is_ok());
            assert!(report.passed);
            assert_eq!(report.tests, ["aoe-x::lib$task_test"]);
        } else {
            assert!(result.is_err());
            assert!(!report.passed);
            if mode == "failed" {
                assert_eq!(report.failed, ["aoe-x::lib$task_test"]);
            } else {
                assert!(report.failed[0].contains("compiler-error"));
            }
        }
    }
    let calls = fs::read_to_string(root.join(".cache/cargo-calls")).unwrap();
    assert_eq!(calls.lines().count(), 3);
    assert!(calls.lines().all(|s| s.starts_with("nextest run ")
        && s.ends_with("-p aoe-x")
        && !s.contains("--workspace")));
}
