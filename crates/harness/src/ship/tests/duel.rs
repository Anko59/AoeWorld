use super::{fixture, run, showcase_security::ENV_LOCK};
use crate::{
    agents::Runtime,
    ship::{
        duel::{Options, run as duel},
        git,
    },
};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};

pub(super) struct Env(Vec<(&'static str, Option<std::ffi::OsString>)>);
impl Env {
    pub(super) fn set(values: &[(&'static str, String)]) -> Self {
        let old = values
            .iter()
            .map(|(k, _)| (*k, std::env::var_os(k)))
            .collect();
        for (key, value) in values {
            unsafe {
                std::env::set_var(key, value);
            }
        }
        Self(old)
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        for (key, value) in &self.0 {
            unsafe {
                match value {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

pub(super) fn setup(root: &Path, mode: &str) -> (Env, Options) {
    fs::create_dir_all(root.join(".cache/bin")).unwrap();
    fs::create_dir_all(root.join(".cache/duel")).unwrap();
    fs::create_dir_all(root.join(".codex")).unwrap();
    fs::create_dir_all(root.join("crates/x/src")).unwrap();
    fs::write(root.join(".gitignore"), ".cache/\n").unwrap();
    fs::write(
        root.join(".codex/hooks.json"),
        r#"{"hooks":{"PreToolUse":[]}}"#,
    )
    .unwrap();
    fs::write(root.join("crates/x/src/lib.rs"), "old\n").unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers=['crates/x']\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/x/Cargo.toml"),
        "[package]\nname='aoe-x'\nversion='0.1.0'\n",
    )
    .unwrap();
    fs::write(
        root.join("Makefile"),
        r#"hooks-check:
	@test -x .git/hooks/pre-commit
duel-check test-unit test-e2e:
	@echo check-tail
	@echo checked >> .cache/checks
	@mkdir -p .cache/duel
	@echo '{"passed":false,"tests":["task_test"],"failed":["task_test"],"untouched":[],"locations":{"task_test":["browser/tests/play.spec.ts",1]}}' > .cache/duel/check.json
	@grep -q green crates/x/src/lib.rs
	@if test "$$DUEL_STUB" = attack && grep -q attack crates/x/src/tests.rs 2>/dev/null; then grep -q healed crates/x/src/lib.rs; fi
	@echo '{"passed":true,"tests":["task_test"],"failed":[],"untouched":[]}' > .cache/duel/check.json
"#,
    ).unwrap();
    run(root, &["add", "-A"]);
    run(root, &["commit", "-qm", "fixture"]);
    // The driver uses the repository identity, without author overrides.
    run(root, &["config", "user.name", "Fixture"]);
    run(root, &["config", "user.email", "fixture@example.com"]);
    let hook = root.join(".git/hooks/pre-commit");
    fs::write(
        &hook,
        "#!/bin/sh\ntest -f .cache/checks || exit 1\necho hook >> .cache/hook-calls\n",
    )
    .unwrap();
    fs::set_permissions(hook, fs::Permissions::from_mode(0o700)).unwrap();
    let script = r#"#!/bin/sh
printf '%s\n' "$AOE_AGENT_ROLE" >> .cache/calls
printf '%s\n' "$*" >> .cache/prompts
case "$AOE_AGENT_ROLE:$DUEL_STUB" in
 tester:timeout) sleep 30; echo late > .cache/late ;;
 tester:exit) echo failure; exit 3 ;;
 tester:staged) echo bad > crates/x/src/lib.rs; git add crates/x/src/lib.rs ;;
 tester:space) mkdir -p "crates/x/tests"; printf '#[test]\nfn task_test() {}\n' > "crates/x/tests/space name.rs" ;;
 tester:browser) mkdir -p browser/tests; echo test > browser/tests/play.spec.ts ;;
 tester:moved-head) git commit -qm moved --allow-empty ;;
 tester:symlink) ln -s lib.rs crates/x/src/tests.rs ;;
 tester:nonutf8) printf bad > "$(printf '\377')" ;;
 tester:none) : ;;
 tester:production) echo bad > crates/x/src/lib.rs ;;
 tester:protected) mkdir -p gates/tests; echo bad > gates/tests/a.rs ;;
 tester:rename) mv crates/x/src/lib.rs crates/x/src/tests.rs ;;
 tester:*)
  if test ! -f crates/x/src/tests.rs; then
    printf '#[test]\nfn task_test() {}\n' > crates/x/src/tests.rs
  elif test "$DUEL_STUB" = attack; then
    echo '// attack' >> crates/x/src/tests.rs
  elif test "$DUEL_STUB" = pass-attack; then
    echo '// attack' >> crates/x/src/tests.rs
  fi ;;
 implementer:exit-impl) echo pending > crates/x/src/lib.rs; exit 4 ;;
 implementer:attack) if grep -q attack crates/x/src/tests.rs; then echo 'green healed' > crates/x/src/lib.rs; else echo green > crates/x/src/lib.rs; fi ;;
 implementer:index-test) echo bad >> crates/x/src/tests.rs; git add crates/x/src/tests.rs; git show HEAD:crates/x/src/tests.rs > crates/x/src/tests.rs ;;
 implementer:symlink-impl) rm crates/x/src/lib.rs; ln -s tests.rs crates/x/src/lib.rs ;;
 implementer:test) echo bad >> crates/x/src/tests.rs ;;
 implementer:protected-impl) echo bad > gates/a.txt ;;
 implementer:new-test) echo bad > crates/x/src/new_tests.rs ;;
 implementer:retry)
  n=$(grep -c implementer .cache/calls)
  if test "$n" = 3; then echo green > crates/x/src/lib.rs; else echo pending > crates/x/src/lib.rs; fi ;;
 implementer:never) echo pending > crates/x/src/lib.rs ;;
 implementer:*) echo green > crates/x/src/lib.rs ;;
esac
echo "$AOE_AGENT_ROLE report"
"#;
    for runtime in ["codex", "claude", "pi"] {
        let path = root.join(".cache/bin").join(runtime);
        fs::write(&path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let env = Env::set(&[
        (
            "PATH",
            format!(
                "{}:{}",
                root.join(".cache/bin").display(),
                std::env::var("PATH").unwrap()
            ),
        ),
        ("DUEL_STUB", mode.into()),
    ]);
    let options = Options {
        runtime: Runtime::Codex,
        model: Some("stub-model".into()),
        effort: Some("high".into()),
        rounds: 2,
        check: None,
        budget: Duration::from_secs(5),
        qualify: None,
        qualify_budget: Duration::from_secs(5),
        qualify_attempts: 2,
    };
    (env, options)
}

#[test]
fn happy_path_commits_tests_before_product_and_stops_on_empty_attack() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for runtime in [Runtime::Codex, Runtime::Claude, Runtime::Pi] {
        let (_temp, root) = fixture("true");
        let (_env, mut options) = setup(&root, "happy");
        options.runtime = runtime;
        let summary = duel(&root, "Make f correct\nDetails", &options).unwrap();
        let phases = git::git(&root, &["log", "-2", "--format=%s"]).unwrap();
        assert_eq!(
            phases.lines().collect::<Vec<_>>(),
            ["Make f correct", "Tests: Make f correct"]
        );
        let messages = git::git(&root, &["log", "-2", "--format=%B"]).unwrap();
        assert!(messages.contains("Tests: Make f correct"));
        assert!(messages.contains("Harness-Phase: tests"));
        assert!(messages.contains("Harness-Phase: implementation"));
        assert!(messages.contains("Co-Authored-By:"));
        assert_eq!(
            git::git(&root, &["log", "-1", "--format=%an <%ae>"]).unwrap(),
            "Fixture <fixture@example.com>"
        );
        assert_eq!(
            fs::read_to_string(root.join(".cache/calls")).unwrap(),
            "tester\nimplementer\ntester\n"
        );
        assert!(summary.contains("red: FAIL") && summary.contains("green: PASS"));
        assert!(summary.contains("tests: task_test"));
        assert!(summary.contains("tester report") && summary.contains("implementer report"));
        assert!(summary.contains("make ship"));
        assert_eq!(
            fs::read_to_string(root.join(".cache/duel/feature.md")).unwrap(),
            summary
        );
        let prompts = fs::read_to_string(root.join(".cache/prompts")).unwrap();
        assert!(prompts.contains("check-tail") && prompts.contains("Attack"));
        assert!(git::subject(&root).is_ok());
        assert_eq!(
            fs::read_to_string(root.join(".cache/hook-calls")).unwrap(),
            "hook\nhook\n"
        );
    }
}

#[test]
fn violations_restore_to_the_last_driver_commit() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (mode, phase, path) in [
        ("production", "tester", "lib.rs"),
        ("staged", "tester", "lib.rs"),
        ("protected", "tester", "gates/tests/a.rs"),
        ("rename", "tester", "lib.rs"),
        ("symlink", "tester", "tests.rs"),
        ("symlink-impl", "implementer", "lib.rs"),
        ("test", "implementer", "tests.rs"),
        ("index-test", "implementer", "tests.rs"),
        ("new-test", "implementer", "new_tests.rs"),
        ("protected-impl", "implementer", "gates/a.txt"),
    ] {
        let (_temp, root) = fixture("true");
        let (_env, options) = setup(&root, mode);
        let before = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
        let error = duel(&root, "Task", &options).unwrap_err();
        assert!(error.contains(phase) && error.contains(path), "{error}");
        assert!(git::subject(&root).is_ok(), "{mode}");
        let after = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
        if phase == "tester" {
            assert_eq!(before, after);
        } else {
            assert_eq!(git::git(&root, &["rev-parse", "HEAD^"]).unwrap(), before);
        }
        assert_eq!(
            fs::read_to_string(root.join("crates/x/src/lib.rs")).unwrap(),
            "old\n"
        );
    }
}

#[test]
fn red_pass_no_tests_timeout_and_failed_runtime_are_not_success() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (mode, expected) in [
        ("happy", "tests already pass"),
        ("none", "the tester wrote no test"),
        ("timeout", "Deadline"),
        ("exit", "Failed"),
    ] {
        let (_temp, root) = fixture("true");
        let (_env, mut options) = setup(&root, mode);
        if mode == "happy" {
            options.check = Some("make test-unit PASS=1".into());
            fs::write(root.join("crates/x/src/lib.rs"), "green\n").unwrap();
            fs::write(root.join(".cache/checks"), "fixture\n").unwrap();
            run(&root, &["commit", "-qam", "already green"]);
        }
        if mode == "timeout" {
            options.budget = Duration::from_millis(100);
        }
        let before = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
        let error = duel(&root, "Task", &options).unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert_eq!(git::git(&root, &["rev-parse", "HEAD"]).unwrap(), before);
        assert!(git::subject(&root).is_ok());
        assert!(!root.join(".cache/late").exists());
    }
}

#[test]
fn green_has_three_attempts_and_keeps_red_commit_and_work_on_exhaustion() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for mode in ["never", "retry"] {
        let (_temp, root) = fixture("true");
        let (_env, options) = setup(&root, mode);
        let result = duel(&root, "Task", &options);
        let calls = fs::read_to_string(root.join(".cache/calls")).unwrap();
        assert_eq!(calls.matches("implementer").count(), 3);
        if mode == "never" {
            assert!(result.unwrap_err().contains("3 attempts"));
            assert!(
                git::git(&root, &["log", "-1", "--format=%B"])
                    .unwrap()
                    .contains("Harness-Phase: tests")
            );
            assert_eq!(
                fs::read_to_string(root.join("crates/x/src/lib.rs")).unwrap(),
                "pending\n"
            );
            assert!(
                fs::read_to_string(root.join(".cache/duel/feature.md"))
                    .unwrap()
                    .contains("tests commit stays")
            );
        } else {
            assert!(result.is_ok());
            assert!(git::subject(&root).is_ok());
        }
    }
}

#[test]
fn passing_attack_is_discarded_without_an_extra_commit() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, options) = setup(&root, "pass-attack");
    let before = git::git(&root, &["rev-list", "--count", "HEAD"])
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!(
        duel(&root, "Task", &options)
            .unwrap()
            .contains("attack tests already pass")
    );
    assert_eq!(
        git::git(&root, &["rev-list", "--count", "HEAD"])
            .unwrap()
            .parse::<usize>()
            .unwrap(),
        before + 2
    );
    assert!(
        !fs::read_to_string(root.join("crates/x/src/tests.rs"))
            .unwrap()
            .contains("attack")
    );
    assert!(git::subject(&root).is_ok());
}

#[test]
fn dirty_tree_and_protected_or_detached_branch_fail_before_launch() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, options) = setup(&root, "happy");
    fs::write(root.join("dirty"), "dirty").unwrap();
    assert!(
        duel(&root, "Task", &options)
            .unwrap_err()
            .contains("commit first")
    );
    fs::remove_file(root.join("dirty")).unwrap();
    run(&root, &["branch", "-f", "dev", "HEAD"]);
    run(&root, &["checkout", "-q", "dev"]);
    assert!(
        duel(&root, "Task", &options)
            .unwrap_err()
            .contains("feature branch")
    );
    run(&root, &["checkout", "-q", "--detach", "feature"]);
    assert!(
        duel(&root, "Task", &options)
            .unwrap_err()
            .contains("detached")
    );
    assert!(!root.join(".cache/calls").exists());
}
