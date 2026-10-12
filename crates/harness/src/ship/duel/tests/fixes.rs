use super::{authored, focus, runner};
use std::fs;

#[test]
fn overridden_build_failure_is_red_but_unknown_failure_is_not() {
    let build = runner::override_report(
        "error: could not compile `aoe-x` (lib test) due to 1 previous error\n",
        false,
    );
    assert!(build.build_failed);
    let unknown = runner::override_report("make: *** [duel-check] Error 1\n", false);
    assert!(!unknown.build_failed);
    assert!(unknown.failed.is_empty());
}

#[test]
fn ambiguous_unqualified_failure_cannot_be_attributed_to_a_changed_module() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("crates/x/src")).unwrap();
    fs::write(
        temp.path().join("crates/x/Cargo.toml"),
        "[package]\nname='aoe-x'\n",
    )
    .unwrap();
    for path in ["tests.rs", "other_tests.rs"] {
        fs::write(
            temp.path().join("crates/x/src").join(path),
            "#[test]\nfn same() {}\n",
        )
        .unwrap();
    }
    let report = focus::CheckReport {
        failed: vec!["same".into()],
        ..Default::default()
    };
    assert!(
        !authored::has_failure(temp.path(), &["crates/x/src/tests.rs".into()], &report).unwrap()
    );
    fs::remove_file(temp.path().join("crates/x/src/other_tests.rs")).unwrap();
    fs::create_dir_all(temp.path().join("crates/y/src")).unwrap();
    fs::write(
        temp.path().join("crates/y/src/tests.rs"),
        "#[test]\nfn same() {}\n",
    )
    .unwrap();
    assert!(
        !authored::has_failure(temp.path(), &["crates/x/src/tests.rs".into()], &report).unwrap()
    );
}

#[test]
fn unit_provenance_matches_real_binary_ids_and_human_override_names() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("crates/x/src")).unwrap();
    fs::write(
        temp.path().join("crates/x/Cargo.toml"),
        "[package]\nname='aoe-x'\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("crates/x/src/tests.rs"),
        "#[test]\nfn own() {}\n",
    )
    .unwrap();
    for (name, expected) in [
        ("aoe-x::aoe_x$tests::own", true),
        ("aoe-x tests::own", true),
        ("aoe-x::unrelated$tests::own", false),
        ("aoe-y::aoe_y$tests::own", false),
        ("aoe-x::aoe_x$other_tests::own", false),
    ] {
        let report = focus::CheckReport {
            failed: vec![name.into()],
            ..Default::default()
        };
        assert_eq!(
            authored::has_failure(temp.path(), &["crates/x/src/tests.rs".into()], &report).unwrap(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn implementer_cannot_touch_inline_tests_in_working_or_staged_product_files() {
    use super::tree;
    use crate::ship::{
        git,
        tests::{fixture, run},
    };
    let original = "\n\npub fn f() { }\n#[cfg(any(unix, test))]\nmod checks {\n    #[test]\n    fn regression() {\n        assert_eq!(1, 1);\n    }\n}\n";
    for staged in [false, true] {
        for (after, forbidden) in [
            (original.replace("        assert_eq!(1, 1);\n", ""), true),
            (
                original.replace("pub fn f() { }", "pub fn f() { let _x = 1; }"),
                false,
            ),
            (original.replace("#[cfg(any(unix, test))]\n", ""), true),
            (
                format!("{original}#[tokio::test]\nasync fn added() {{}}\n"),
                true,
            ),
        ] {
            let (_temp, root) = fixture("true");
            fs::create_dir_all(root.join("crates/x/src")).unwrap();
            let path = root.join("crates/x/src/lib.rs");
            fs::write(&path, original).unwrap();
            run(&root, &["add", "crates/x/src/lib.rs"]);
            run(&root, &["commit", "-qm", "inline test fixture"]);
            let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
            fs::write(&path, &after).unwrap();
            if staged {
                run(&root, &["add", "crates/x/src/lib.rs"]);
                fs::write(&path, original).unwrap();
            }
            let result = tree::validate(&root, false, &head, "feature");
            if forbidden {
                let error = result.unwrap_err();
                assert!(
                    error.contains("inline test") && error.contains("restored"),
                    "{error}"
                );
                assert_eq!(fs::read_to_string(&path).unwrap(), original);
                assert!(git::subject(&root).is_ok());
            } else {
                assert_eq!(result.unwrap(), ["crates/x/src/lib.rs"]);
            }
        }
    }
}

#[test]
fn real_playwright_paths_attribute_nested_suite_failures_to_changed_spec() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("report.json");
    fs::write(&path, r#"{"errors":[],"suites":[{"title":"tests/foo.spec.ts","file":"tests/foo.spec.ts","suites":[{"title":"nested","file":"tests/foo.spec.ts","specs":[{"file":"tests/foo.spec.ts","line":12,"title":"works","tests":[{"projectName":"webgpu","status":"unexpected"}]}]}]}]}"#).unwrap();
    let report = focus::browser_results(&path).unwrap();
    assert_eq!(
        report.locations[&report.failed[0]],
        ("browser/tests/foo.spec.ts".into(), 12)
    );
    assert!(
        authored::has_failure(temp.path(), &["browser/tests/foo.spec.ts".into()], &report).unwrap()
    );
}

#[test]
fn oversized_playwright_json_is_unavailable_before_parsing() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("report.json");
    for size in [1024 * 1024 + 1, 16 * 1024 * 1024 + 1] {
        fs::write(&path, vec![b' '; size]).unwrap();
        let error = focus::browser_results(&path).unwrap_err();
        assert!(error.contains("limit exceeded"), "{error}");
    }
}

#[test]
fn explicit_exit_status_overrides_report_status_and_unavailability() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".cache/duel")).unwrap();
    let options =
        super::Options::from_env(|key| (key == "DUEL_RUNTIME").then(|| "codex".into())).unwrap();
    for exit in [0, 1] {
        fs::write(temp.path().join("Makefile"), format!("check:\n\t@echo '{{\"passed\":{},\"unavailable\":\"metadata\",\"tests\":[\"named\"],\"failed\":[],\"untouched\":[]}}' > .cache/duel/check.json\n\t@exit {exit}\n", exit != 0)).unwrap();
        let (report, _) = runner::check(temp.path(), &options, "make check", false).unwrap();
        assert_eq!(report.passed, exit == 0);
        assert!(report.unavailable.is_none());
        assert_eq!(report.tests, ["named"]);
    }
}

#[test]
fn retained_playwright_output_rejects_growth_before_writing_excess_bytes() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("report.json");
    let mut output = super::output::JsonOutput::create(&path, "Playwright").unwrap();
    let chunk = vec![b'x'; 1024 * 1024];
    output.write_all(&chunk).unwrap();
    assert!(
        output
            .write_all(b"x")
            .unwrap_err()
            .to_string()
            .contains("line limit exceeded")
    );
    assert_eq!(fs::metadata(&path).unwrap().len(), 1024 * 1024);
    let mut output = super::output::JsonOutput::create(&path, "Playwright").unwrap();
    let mut chunk = chunk;
    chunk[0] = b'\n';
    for _ in 0..16 {
        output.write_all(&chunk).unwrap();
    }
    assert!(
        output
            .write_all(b"\n")
            .unwrap_err()
            .to_string()
            .contains("total limit exceeded")
    );
    assert_eq!(fs::metadata(&path).unwrap().len(), 16 * 1024 * 1024);
}
