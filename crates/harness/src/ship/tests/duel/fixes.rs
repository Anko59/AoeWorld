use super::super::{duel::setup, fixture, run, showcase_security::ENV_LOCK};
use crate::ship::{duel::run as duel, git};
use std::fs;

#[test]
fn malformed_product_tokens_reach_focused_check_and_retry() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, mut options) = setup(&root, "happy");
    options.rounds = 1;
    fs::write(root.join(".cache/checks"), "fixture\n").unwrap();
    fs::write(root.join("crates/x/src/lib.rs"), "pub fn f() {}\n").unwrap();
    run(&root, &["add", "crates/x/src/lib.rs"]);
    run(&root, &["commit", "-qm", "valid product fixture"]);
    let runtime = root.join(".cache/bin/codex");
    let script = fs::read_to_string(&runtime).unwrap().replace(
        "implementer:*) echo green > crates/x/src/lib.rs",
        "implementer:*) if test $(grep -c implementer .cache/calls) = 1; then echo 'pub fn f() {' > crates/x/src/lib.rs; else echo 'pub fn f() {} // green' > crates/x/src/lib.rs; fi",
    );
    fs::write(runtime, script).unwrap();
    let summary = duel(&root, "Task", &options).unwrap();
    assert!(summary.contains("green: FAIL (attempt 1)"), "{summary}");
    assert!(summary.contains("green: PASS (attempt 2)"), "{summary}");
    assert_eq!(
        fs::read_to_string(root.join(".cache/calls")).unwrap(),
        "tester\nimplementer\nimplementer\n"
    );
    assert_eq!(
        fs::read_to_string(root.join(".cache/checks"))
            .unwrap()
            .matches("checked")
            .count(),
        3
    );
    assert!(git::subject(&root).is_ok());
}

#[test]
fn cargo_tool_failures_are_unavailable_but_package_compile_errors_are_red() {
    use super::super::duel::Env;
    use crate::ship::duel::focus;
    use std::os::unix::fs::PermissionsExt;
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, _) = setup(&root, "happy");
    let base = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let _base = Env::set(&[
        ("HARNESS_DUEL_BASE", base.clone()),
        ("HARNESS_DUEL_REVISION", "".into()),
    ]);
    fs::write(
        root.join("crates/x/src/tests.rs"),
        "#[test]\nfn added() {}\n",
    )
    .unwrap();
    let cargo = root.join(".cache/bin/cargo");
    for (diagnostic, compile) in [
        ("error: no such command: `nextest`", false),
        (
            "error: unexpected argument '--message-format-version'",
            false,
        ),
        (
            "error: could not compile `aoe-other` (lib test) due to 1 previous error",
            false,
        ),
        (
            "error: could not compile `aoe-x` (lib test) due to 1 previous error",
            true,
        ),
        (
            r#"{"reason":"compiler-message","package_id":"path+file:///checkout/crates/x#aoe-x@0.1.0","message":{"level":"error"}}"#,
            true,
        ),
        (
            r#"{"reason":"compiler-message","package_id":"path+file:///checkout/crates/other#aoe-other@0.1.0","message":{"level":"error"}}"#,
            false,
        ),
    ] {
        let diagnostic = diagnostic.replace('\'', "'\\''");
        fs::write(
            &cargo,
            format!("#!/bin/sh\nprintf '%s\\n' '{diagnostic}' >&2\nexit 1\n"),
        )
        .unwrap();
        fs::set_permissions(&cargo, fs::Permissions::from_mode(0o700)).unwrap();
        let result = focus::execute(&root);
        let report: focus::CheckReport =
            serde_json::from_slice(&fs::read(root.join(".cache/duel/check.json")).unwrap())
                .unwrap();
        assert!(result.is_err());
        if compile {
            assert!(report.build_failed && !report.passed && report.unavailable.is_none());
        } else {
            let error = result.unwrap_err();
            assert!(error.contains("package check unavailable"), "{error}");
            assert!(report.unavailable.is_some());
            assert!(!report.build_failed && !report.passed);
        }
        assert_eq!(git::git(&root, &["rev-parse", "HEAD"]).unwrap(), base);
    }
}

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
        fs::write(root.join("Makefile"), "hooks-check:\n\t@test -x .git/hooks/pre-commit\nduel-check fuzz-smoke:\n\t@echo '{\"passed\":false,\"build_failed\":true,\"tests\":[],\"failed\":[],\"untouched\":[]}' > .cache/duel/check.json\n\t@exit 1\n").unwrap();
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

#[test]
fn explicit_fuzz_check_uses_exit_code_without_test_names_or_duel_report() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for green in [true, false] {
        let (_temp, root) = fixture("true");
        let (_env, mut options) = setup(&root, "happy");
        options.rounds = 1;
        options.check = Some("make fuzz-smoke".into());
        fs::write(root.join(".cache/checks"), "fixture\n").unwrap();
        fs::create_dir_all(root.join("fuzz/fuzz_targets")).unwrap();
        fs::write(
            root.join("fuzz/fuzz_targets/a.rs"),
            "// original property\n",
        )
        .unwrap();
        fs::write(root.join("Makefile"), format!("hooks-check:\n\t@test -x .git/hooks/pre-commit\nfuzz-smoke:\n\t@mkdir -p reports/fuzz\n\t@echo '{{\"result\":\"FAIL\"}}' > reports/fuzz/smoke.json\n\t@echo fuzz-runtime-crash\n\t@{}\n", if green { "grep -q green crates/x/src/lib.rs" } else { "exit 1" })).unwrap();
        fs::write(root.join(".gitignore"), ".cache/\nreports/\n").unwrap();
        run(
            &root,
            &["add", "Makefile", ".gitignore", "fuzz/fuzz_targets/a.rs"],
        );
        run(&root, &["commit", "-qm", "runtime fuzz fixture"]);
        let runtime = root.join(".cache/bin/codex");
        let script = fs::read_to_string(&runtime).unwrap().replace(
            "printf '#[test]\\nfn task_test() {}\\n' > crates/x/src/tests.rs",
            "echo '// added-property' >> fuzz/fuzz_targets/a.rs",
        );
        fs::write(runtime, script).unwrap();
        let result = duel(&root, "Task", &options);
        let calls = fs::read_to_string(root.join(".cache/calls")).unwrap();
        assert_eq!(
            calls.matches("implementer").count(),
            if green { 1 } else { 3 }
        );
        assert!(
            fs::read_to_string(root.join("fuzz/fuzz_targets/a.rs"))
                .unwrap()
                .contains("added-property")
        );
        if green {
            let summary = result.unwrap();
            assert!(summary.contains("red: FAIL") && summary.contains("green: PASS"));
            assert!(git::subject(&root).is_ok());
        } else {
            assert!(result.unwrap_err().contains("3 attempts"));
            assert!(
                git::git(&root, &["log", "-1", "--format=%B"])
                    .unwrap()
                    .contains("Harness-Phase: tests")
            );
        }
    }
}

#[test]
fn browser_checks_reuse_dependencies_but_rebuild_product_each_time() {
    use super::super::duel::Env;
    use crate::ship::duel::{focus, stages};
    use std::os::unix::fs::PermissionsExt;
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    let (_env, _) = setup(&root, "happy");
    fs::create_dir_all(root.join("browser")).unwrap();
    for program in ["cargo", "wasm-bindgen", "make"] {
        let path = root.join(".cache/bin").join(program);
        fs::write(&path, "#!/bin/sh\necho \"$(basename \"$0\") $*\" >> .cache/browser-build-calls\nmkdir -p browser/node_modules\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let _target = Env::set(&[(
        "CARGO_TARGET_DIR",
        root.join("target").display().to_string(),
    )]);
    let scope = focus::Scope {
        specs: vec!["browser/tests/foo.spec.ts".into()],
        packages: vec![],
        wasm: vec![],
        manifests: vec![],
        paths: vec![],
    };
    fs::write(
        root.join(".cache/duel/scope.json"),
        serde_json::to_vec(&scope).unwrap(),
    )
    .unwrap();
    let _base = Env::set(&[(
        "HARNESS_DUEL_BASE",
        git::git(&root, &["rev-parse", "HEAD"]).unwrap(),
    )]);
    // No server binary: both attempts stop after prerequisites, without launching Docker.
    for _ in 0..2 {
        fs::write(
            root.join(".cache/duel/check.json"),
            serde_json::to_vec(&focus::CheckReport {
                passed: true,
                ..Default::default()
            })
            .unwrap(),
        )
        .unwrap();
        assert!(stages::browser(&root).is_err());
    }
    let calls = fs::read_to_string(root.join(".cache/browser-build-calls")).unwrap();
    assert_eq!(calls.matches("make browser-deps").count(), 1, "{calls}");
    assert_eq!(calls.matches("cargo build --locked --release").count(), 2);
}
