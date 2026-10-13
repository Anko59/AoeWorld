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
