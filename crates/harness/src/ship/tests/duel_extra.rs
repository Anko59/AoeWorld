use super::{
    duel::{Env, setup},
    fixture, run,
    showcase_security::ENV_LOCK,
};
use crate::ship::{duel::run as duel, git};
use std::{fs, time::Duration};

#[path = "duel/fixes.rs"]
mod fixes;

#[test]
fn unrelated_red_failures_are_refused_without_launching_the_implementer() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    {
        let (_temp, root) = fixture("true");
        let (_env, mut options) = setup(&root, "happy");
        options.rounds = 1;
        fs::write(root.join(".cache/checks"), "fixture\n").unwrap();
        fs::write(root.join("Makefile"), "hooks-check:\n\t@test -x .git/hooks/pre-commit\nduel-check:\n\t@echo '{\"passed\":false,\"tests\":[\"aoe-x::other$$existing\"],\"failed\":[\"aoe-x::other$$existing\"],\"untouched\":[]}' > .cache/duel/check.json\n\t@exit 1\n").unwrap();
        run(&root, &["add", "Makefile"]);
        run(&root, &["commit", "-qm", "unrelated check"]);
        let before = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
        let error = duel(&root, "Task", &options).unwrap_err();
        assert!(
            error.contains("tester-authored") && error.contains("aoe-x::other$existing"),
            "{error}"
        );
        assert_eq!(
            fs::read_to_string(root.join(".cache/calls")).unwrap(),
            "tester\n"
        );
        assert_eq!(git::git(&root, &["rev-parse", "HEAD"]).unwrap(), before);
        assert!(git::subject(&root).is_ok());
    }
}

#[test]
fn red_matches_integration_binaries_and_unit_module_paths_in_changed_files() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (path, failing, accepted) in [
        ("crates/x/tests/changed.rs", "aoe-x::changed$existing", true),
        ("crates/x/tests/changed.rs", "aoe-x::other$existing", false),
        (
            "crates/x/tests/changed.rs",
            "aoe-y::changed$existing",
            false,
        ),
        (
            "crates/x/src/foo/tests.rs",
            "aoe-x::lib$foo::tests::existing",
            true,
        ),
        (
            "crates/x/src/foo/tests.rs",
            "aoe-x::lib$bar::tests::existing",
            false,
        ),
        (
            "crates/x/src/foo_tests.rs",
            "aoe-x::lib$foo_tests::existing",
            true,
        ),
    ] {
        let (_temp, root) = fixture("true");
        let (_env, mut options) = setup(&root, "never");
        options.rounds = 1;
        fs::write(root.join(".cache/checks"), "fixture\n").unwrap();
        fs::write(
            root.join("crates/x/Cargo.toml"),
            "[package]\nname='aoe-x'\n",
        )
        .unwrap();
        let escaped_failure = failing.replace('$', "$$");
        fs::write(root.join("Makefile"), format!("hooks-check:\n\t@test -x .git/hooks/pre-commit\nduel-check:\n\t@echo '{{\"passed\":false,\"tests\":[\"{escaped_failure}\"],\"failed\":[\"{escaped_failure}\"],\"untouched\":[]}}' > .cache/duel/check.json\n\t@exit 1\n")).unwrap();
        run(&root, &["add", "Makefile", "crates/x/Cargo.toml"]);
        run(&root, &["commit", "-qm", "identified check"]);
        let runtime = root.join(".cache/bin/codex");
        let script = fs::read_to_string(&runtime).unwrap().replace(
            "printf '#[test]\\nfn task_test() {}\\n' > crates/x/src/tests.rs",
            &format!(
                "mkdir -p {}; printf '#[test]\\nfn existing() {{}}\\n' > {path}",
                root.join(path).parent().unwrap().display()
            ),
        );
        fs::write(runtime, script).unwrap();
        let error = duel(&root, "Task", &options).unwrap_err();
        let calls = fs::read_to_string(root.join(".cache/calls")).unwrap();
        if accepted {
            assert!(error.contains("3 attempts"), "{error}");
            assert_eq!(calls.matches("implementer").count(), 3);
        } else {
            assert!(
                error.contains("tester-authored") && error.contains(failing),
                "{error}"
            );
            assert_eq!(calls, "tester\n");
        }
    }
}

#[test]
fn test_compile_defect_report_stops_after_one_implementer_and_keeps_tests() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, mut options) = setup(&root, "happy");
    options.rounds = 1;
    fs::write(root.join(".cache/checks"), "fixture\n").unwrap();
    fs::write(root.join("Makefile"), "hooks-check:\n\t@test -x .git/hooks/pre-commit\nduel-check:\n\t@echo '{\"passed\":false,\"build_failed\":true,\"tests\":[],\"failed\":[],\"untouched\":[]}' > .cache/duel/check.json\n\t@echo syntax-error\n\t@exit 1\n").unwrap();
    run(&root, &["add", "Makefile"]);
    run(&root, &["commit", "-qm", "build check"]);
    let runtime = root.join(".cache/bin/codex");
    let script = fs::read_to_string(&runtime).unwrap().replace(
        "implementer:*) echo green > crates/x/src/lib.rs ;;",
        "implementer:*) echo 'DUEL_TEST_BUILD_ERROR: syntax error in tests.rs'; exit 0 ;;",
    );
    fs::write(runtime, script).unwrap();
    let error = duel(&root, "Task", &options).unwrap_err();
    assert!(error.contains("syntax error in tests.rs"), "{error}");
    assert_eq!(
        fs::read_to_string(root.join(".cache/calls")).unwrap(),
        "tester\nimplementer\n"
    );
    assert!(
        git::git(&root, &["log", "-1", "--format=%B"])
            .unwrap()
            .contains("Harness-Phase: tests")
    );
    let prompts = fs::read_to_string(root.join(".cache/prompts")).unwrap();
    assert!(prompts.contains("other than missing production code"));
    assert!(prompts.contains("DUEL_TEST_BUILD_ERROR:"));
}

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
hooks-check:
	@test -x .git/hooks/pre-commit
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
if test "$DUEL_STUB" = build; then printf '%s\n' 'error: could not compile `aoe-x` (lib test): compiler-error' >&2; exit 101; fi
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

#[test]
fn retained_nextest_output_overflow_is_unavailable_and_bounded_on_disk() {
    use crate::ship::duel::focus;
    use std::os::unix::fs::PermissionsExt;
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, _) = setup(&root, "happy");
    let cargo = root.join(".cache/bin/cargo");
    fs::write(&cargo, "#!/bin/sh\nif test \"$DUEL_STUB\" = line; then head -c 1048577 /dev/zero; else i=0; while test \"$i\" -lt 17; do head -c 1048575 /dev/zero; printf '\\n'; i=$((i+1)); done; fi\n").unwrap();
    fs::set_permissions(cargo, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        root.join("crates/x/Cargo.toml"),
        "[package]\nname='aoe-x'\n",
    )
    .unwrap();
    fs::write(root.join(".cache/checks"), "fixture\n").unwrap();
    run(&root, &["add", "crates/x/Cargo.toml"]);
    run(&root, &["commit", "-qm", "package"]);
    let base = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    fs::write(
        root.join("crates/x/src/tests.rs"),
        "#[test]\nfn task_test() {}\n",
    )
    .unwrap();
    let _base = Env::set(&[
        ("HARNESS_DUEL_BASE", base),
        ("HARNESS_DUEL_REVISION", "".into()),
    ]);
    for mode in ["line", "total"] {
        let _mode = Env::set(&[("DUEL_STUB", mode.into())]);
        assert!(focus::execute(&root).is_err());
        let report: focus::CheckReport =
            serde_json::from_slice(&fs::read(root.join(".cache/duel/check.json")).unwrap())
                .unwrap();
        let reason = report.unavailable.unwrap();
        assert!(
            reason.contains("nextest JSON") && reason.contains("limit"),
            "{reason}"
        );
        assert!(!report.passed && !report.build_failed);
        let logs: Vec<_> = fs::read_dir(root.join(".cache/duel"))
            .unwrap()
            .map(|entry| entry.unwrap().path().join("nextest.jsonl"))
            .filter(|path| path.is_file())
            .collect();
        assert_eq!(logs.len(), if mode == "line" { 1 } else { 2 });
        for log in logs {
            let length = fs::metadata(log).unwrap().len();
            assert!(length <= 16 * 1024 * 1024, "{length}");
            if mode == "line" {
                assert!(length <= 1024 * 1024, "{length}");
            }
        }
    }
}
