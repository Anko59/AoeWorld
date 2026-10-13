use super::super::{duel::setup, fixture, run, showcase_security::ENV_LOCK};
use crate::ship::{duel::run as duel, git};
use std::fs;

/// `duel-check` is unavailable until `.cache/ready` exists, then red until `green` is in lib.rs.
fn gated_check(root: &std::path::Path) {
    let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
    let unavailable = "\t@if test ! -f .cache/ready; then echo '{\"passed\":false,\"unavailable\":\"runner missing\",\"tests\":[],\"failed\":[],\"untouched\":[]}' > .cache/duel/check.json; exit 1; fi\n";
    fs::write(
        root.join("Makefile"),
        makefile.replace(
            "\t@echo checked >> .cache/checks\n\t@mkdir -p .cache/duel\n",
            &format!("\t@echo checked >> .cache/checks\n\t@mkdir -p .cache/duel\n{unavailable}"),
        ),
    )
    .unwrap();
    fs::write(root.join(".cache/checks"), "fixture\n").unwrap();
    run(root, &["commit", "-qam", "gated check"]);
}

#[test]
fn an_unavailable_check_keeps_the_testers_work_and_a_rerun_continues_from_it() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, mut options) = setup(&root, "happy");
    gated_check(&root);
    options.rounds = 1;
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let error = duel(&root, "Task", &options).unwrap_err();
    assert!(error.contains("runner missing"), "{error}");
    assert!(
        error.contains("kept, uncommitted") && error.contains("DUEL_CHECK="),
        "{error}"
    );
    assert!(!error.contains("restored"), "{error}");
    assert_eq!(git::git(&root, &["rev-parse", "HEAD"]).unwrap(), head);
    let tests = root.join("crates/x/src/tests.rs");
    assert!(tests.is_file(), "the tester's test file was restored away");
    assert!(git::subject(&root).is_err(), "the tests stay uncommitted");
    let marker = root.join(".git/aoe-ship/duel-kept/feature.json");
    let record = fs::read_to_string(&marker).unwrap();
    assert!(
        record.contains("crates/x/src/tests.rs") && record.contains(&head),
        "{record}"
    );

    // The rerun starts from the kept tests: no tester, straight to the red check.
    fs::write(root.join(".cache/ready"), "").unwrap();
    let summary = duel(&root, "Task", &options).unwrap();
    assert!(
        summary.contains("the tester was skipped") && summary.contains("red: FAIL"),
        "{summary}"
    );
    assert_eq!(
        fs::read_to_string(root.join(".cache/calls")).unwrap(),
        "tester\nimplementer\n"
    );
    assert!(git::subject(&root).is_ok());
    assert!(!marker.exists(), "the committed round's record is removed");
    assert!(
        git::git(&root, &["log", "-2", "--format=%B"])
            .unwrap()
            .contains("Harness-Phase: tests")
    );
}

#[test]
fn only_test_file_changes_resume_and_any_other_change_still_refuses() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, options) = setup(&root, "happy");
    fs::write(
        root.join("crates/x/src/tests.rs"),
        "#[test]\nfn task_test() {}\n",
    )
    .unwrap();
    fs::write(root.join("crates/x/src/lib.rs"), "changed\n").unwrap();
    let error = duel(&root, "Task", &options).unwrap_err();
    assert!(error.contains("commit first"), "{error}");
    assert!(!root.join(".cache/calls").exists());
}

/// Let the gated check also pass when `.cache/pass` exists, so red can be "already green".
fn passable_check(root: &std::path::Path) {
    let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
    fs::write(
        root.join("Makefile"),
        makefile.replace(
            "@grep -q green crates/x/src/lib.rs",
            "@test -f .cache/pass || grep -q green crates/x/src/lib.rs",
        ),
    )
    .unwrap();
    run(root, &["commit", "-qam", "passable check"]);
}

#[test]
fn hand_made_test_changes_without_a_record_are_refused_and_untouched() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, options) = setup(&root, "happy");
    let mine = "#[test]\nfn mine() {}\n";
    fs::write(root.join("crates/x/src/tests.rs"), mine).unwrap();
    let error = duel(&root, "Task", &options).unwrap_err();
    assert!(error.contains("commit first"), "{error}");
    assert_eq!(
        fs::read_to_string(root.join("crates/x/src/tests.rs")).unwrap(),
        mine
    );
    assert!(!root.join(".cache/calls").exists());
}

#[test]
fn a_kept_file_changed_since_the_record_is_refused_and_untouched() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, mut options) = setup(&root, "happy");
    gated_check(&root);
    options.rounds = 1;
    duel(&root, "Task", &options).unwrap_err();
    let tests = root.join("crates/x/src/tests.rs");
    let edited = format!("{}// by hand\n", fs::read_to_string(&tests).unwrap());
    fs::write(&tests, &edited).unwrap();
    fs::write(root.join(".cache/ready"), "").unwrap();
    let error = duel(&root, "Task", &options).unwrap_err();
    assert!(
        error.contains("commit or stash") && error.contains("crates/x/src/tests.rs changed since"),
        "{error}"
    );
    assert_eq!(fs::read_to_string(&tests).unwrap(), edited);
    assert_eq!(
        fs::read_to_string(root.join(".cache/calls")).unwrap(),
        "tester\n"
    );
}

#[test]
fn a_resumed_round_that_ends_in_tests_already_pass_leaves_the_files_in_place() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, mut options) = setup(&root, "happy");
    gated_check(&root);
    passable_check(&root);
    options.rounds = 1;
    duel(&root, "Task", &options).unwrap_err();
    let tests = root.join("crates/x/src/tests.rs");
    let kept = fs::read_to_string(&tests).unwrap();
    fs::write(root.join(".cache/ready"), "").unwrap();
    fs::write(root.join(".cache/pass"), "").unwrap();
    let error = duel(&root, "Task", &options).unwrap_err();
    assert!(error.contains("tests already pass at red"), "{error}");
    assert!(
        error.contains("(kept)") && !error.contains("restored"),
        "{error}"
    );
    assert_eq!(fs::read_to_string(&tests).unwrap(), kept);
    assert_eq!(
        fs::read_to_string(root.join(".cache/calls")).unwrap(),
        "tester\n"
    );
    // The duel ended, so the record is gone: the leftover tests need a commit or a stash.
    assert!(
        duel(&root, "Task", &options)
            .unwrap_err()
            .contains("commit first")
    );
    assert_eq!(fs::read_to_string(&tests).unwrap(), kept);
}
