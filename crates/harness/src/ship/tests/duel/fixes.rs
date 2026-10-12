use super::super::{duel::setup, fixture, run, showcase_security::ENV_LOCK};
use crate::ship::{duel::run as duel, git};
use std::fs;

#[test]
fn unsupported_fuzz_scope_stops_before_implementer_and_override_accepts_build_red() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for overridden in [false, true] {
        let (_temp, root) = fixture("true");
        let (_env, mut options) = setup(&root, "happy");
        options.rounds = 1;
        fs::write(root.join(".cache/checks"), "fixture\n").unwrap();
        fs::write(root.join("Makefile"), "duel-check fuzz-smoke:\n\t@echo '{\"passed\":false,\"build_failed\":true,\"tests\":[],\"failed\":[],\"untouched\":[]}' > .cache/duel/check.json\n\t@exit 1\n").unwrap();
        run(&root, &["add", "Makefile"]);
        run(&root, &["commit", "-qm", "fuzz check"]);
        let runtime = root.join(".cache/bin/codex");
        let script = fs::read_to_string(&runtime).unwrap().replace(
            "printf '#[test]\\nfn task_test() {}\\n' > crates/x/src/tests.rs",
            "mkdir -p fuzz/fuzz_targets; echo fuzz > fuzz/fuzz_targets/a.rs",
        );
        fs::write(runtime, script).unwrap();
        let before = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
        if overridden {
            options.check = Some("make fuzz-smoke".into());
        }
        let error = duel(&root, "Task", &options).unwrap_err();
        let calls = fs::read_to_string(root.join(".cache/calls")).unwrap();
        if overridden {
            assert!(error.contains("3 attempts"), "{error}");
            assert_eq!(calls.matches("implementer").count(), 3);
        } else {
            assert!(
                error.contains("set DUEL_CHECK") && error.contains("make fuzz-smoke"),
                "{error}"
            );
            assert_eq!(calls, "tester\n");
            assert_eq!(git::git(&root, &["rev-parse", "HEAD"]).unwrap(), before);
            assert!(git::subject(&root).is_ok());
        }
    }
}

#[test]
fn later_unrelated_red_is_dropped_and_its_failure_is_named() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, options) = setup(&root, "attack");
    fs::write(root.join(".cache/checks"), "fixture\n").unwrap();
    let make = fs::read_to_string(root.join("Makefile")).unwrap().replace(
        "then grep -q healed crates/x/src/lib.rs; fi",
        "then echo '{\"passed\":false,\"tests\":[\"other_failure\"],\"failed\":[\"other_failure\"],\"untouched\":[]}' > .cache/duel/check.json; exit 1; fi",
    );
    fs::write(root.join("Makefile"), make).unwrap();
    run(&root, &["add", "Makefile"]);
    run(&root, &["commit", "-qm", "unrelated attack check"]);
    let summary = duel(&root, "Task", &options).unwrap();
    assert!(summary.contains("unrelated failures: other_failure"));
    assert_eq!(summary.matches("Tests commit:").count(), 1);
    assert_eq!(
        fs::read_to_string(root.join(".cache/calls"))
            .unwrap()
            .matches("implementer")
            .count(),
        1
    );
    assert!(git::subject(&root).is_ok());
}
