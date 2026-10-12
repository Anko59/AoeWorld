use super::{focus, tree};
use std::fs;

#[test]
fn green_requires_red_failures_or_all_build_blocked_test_definitions_to_pass() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("crates/x/tests")).unwrap();
    fs::write(
        root.join("crates/x/Cargo.toml"),
        "[package]\nname='aoe-x'\n",
    )
    .unwrap();
    let paths = vec!["crates/x/tests/check.rs".into()];
    fs::write(
        root.join(&paths[0]),
        "#[test]\nfn first() {}\nmod nested { #[test] fn second() {} }\n",
    )
    .unwrap();
    let red = focus::CheckReport {
        failed: vec![
            "aoe-x::check$first".into(),
            "aoe-x::check$nested::second".into(),
        ],
        ..Default::default()
    };
    let mut green = focus::rust_results(
        "{\"type\":\"test\",\"event\":\"ok\",\"name\":\"aoe-x::check$first\"}\n{\"type\":\"test\",\"event\":\"ignored\",\"name\":\"aoe-x::check$nested::second\"}\n",
    );
    green.passed = true;
    let error = super::green::validate(root, &paths, None, &red, &green).unwrap_err();
    assert_eq!(
        error,
        "the tester's tests did not run: aoe-x::check$nested::second"
    );
    let build_red = focus::CheckReport {
        build_failed: true,
        ..Default::default()
    };
    assert!(
        super::green::validate(root, &paths, None, &build_red, &green)
            .unwrap_err()
            .contains("check.rs::nested::second")
    );
    green.tests.push("aoe-y::check$nested::second".into());
    assert!(super::green::validate(root, &paths, None, &build_red, &green).is_err());
    green
        .tests
        .push("aoe-x::check$other::nested::second".into());
    assert!(super::green::validate(root, &paths, None, &build_red, &green).is_err());
    green.tests.push("aoe-x::check$nested::second".into());
    green.failed.push("aoe-x::check$nested::second".into());
    assert!(super::green::validate(root, &paths, None, &red, &green).is_err());
    assert!(super::green::validate(root, &paths, None, &build_red, &green).is_err());
    green.failed.clear();
    assert!(super::green::validate(root, &paths, None, &red, &green).is_ok());
    assert!(super::green::validate(root, &paths, None, &build_red, &green).is_ok());
    fs::write(
        root.join(&paths[0]),
        "#[test] fn first() {}\nmod nested { #[test] fn first() {} }\n",
    )
    .unwrap();
    let alias = focus::CheckReport {
        tests: vec!["aoe-x::check$nested::first".into()],
        ..Default::default()
    };
    assert!(
        super::green::validate(root, &paths, None, &build_red, &alias)
            .unwrap_err()
            .contains("check.rs::first")
    );
    fs::write(root.join(&paths[0]), "fn helper() {}\n").unwrap();
    assert!(
        super::green::validate(root, &paths, None, &build_red, &green)
            .unwrap_err()
            .contains(&paths[0])
    );
    fs::create_dir_all(root.join("browser/tests")).unwrap();
    fs::write(
        root.join("browser/tests/play.spec.ts"),
        "test('works', () => {});\n",
    )
    .unwrap();
    let browser_paths = vec!["browser/tests/play.spec.ts".into()];
    let output = root.join("playwright.json");
    for (status, expected) in [("skipped", false), ("expected", true)] {
        fs::write(&output, format!(r#"{{"errors":[],"suites":[{{"specs":[{{"file":"play.spec.ts","line":1,"title":"works","tests":[{{"projectName":"canvas","status":"{status}"}}]}}]}}]}}"#)).unwrap();
        let browser = focus::browser_results(&output).unwrap();
        assert_eq!(
            super::green::validate(root, &browser_paths, None, &build_red, &browser).is_ok(),
            expected
        );
        let browser_red = focus::CheckReport {
            failed: vec!["play.spec.ts::works [canvas]".into()],
            ..Default::default()
        };
        assert_eq!(
            super::green::validate(root, &browser_paths, None, &browser_red, &browser).is_ok(),
            expected
        );
    }
    fs::write(
        root.join(&browser_paths[0]),
        "test('works', () => {}); test('missing', () => {});\n",
    )
    .unwrap();
    let browser = focus::browser_results(&output).unwrap();
    assert!(
        super::green::validate(root, &browser_paths, None, &build_red, &browser)
            .unwrap_err()
            .contains("missing")
    );
}

#[test]
fn fixture_scopes_run_browser_specs_and_separate_fuzz_scopes_need_an_override() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("browser/tests/nested")).unwrap();
    for path in [
        "browser/tests/play.spec.ts",
        "browser/tests/nested/a.test.ts",
    ] {
        fs::write(temp.path().join(path), "test('a', () => {});").unwrap();
    }
    let scope = focus::Scope::from_paths(temp.path(), &["browser/tests/fixtures/seed.json".into()])
        .unwrap();
    assert!(scope.packages.is_empty());
    assert_eq!(
        scope.specs,
        [
            "browser/tests/nested/a.test.ts",
            "browser/tests/play.spec.ts"
        ]
    );
    fs::create_dir_all(temp.path().join("fuzz/fuzz_targets")).unwrap();
    fs::write(
        temp.path().join("fuzz/Cargo.toml"),
        "[package]\nname='fuzz'\n[workspace]\n",
    )
    .unwrap();
    let error =
        focus::Scope::from_paths(temp.path(), &["fuzz/fuzz_targets/a.rs".into()]).unwrap_err();
    assert!(
        error.contains("set DUEL_CHECK") && error.contains("make fuzz-smoke"),
        "{error}"
    );
    let error =
        focus::Scope::from_paths(temp.path(), &["crates/core/tests/data.json".into()]).unwrap_err();
    assert!(error.contains("set DUEL_CHECK"), "{error}");
}

#[test]
fn no_failures_need_no_baseline_provenance_scan() {
    let temp = tempfile::tempdir().unwrap();
    let scope = focus::Scope {
        packages: vec!["absent".into()],
        specs: vec![],
        paths: vec![],
        manifests: vec!["absent/Cargo.toml".into()],
    };
    assert!(
        super::baseline::untouched(temp.path(), "unavailable", &scope, &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn focused_scope_selects_owning_packages_and_exact_specs_including_mixed_changes() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("Cargo.toml"),
        "[workspace]\nmembers=['crates/core','crates/other']\n",
    )
    .unwrap();
    for (dir, name) in [("core", "aoe-core"), ("other", "aoe-other")] {
        fs::create_dir_all(temp.path().join(format!("crates/{dir}/tests"))).unwrap();
        fs::write(
            temp.path().join(format!("crates/{dir}/Cargo.toml")),
            format!("[package]\nname = '{name}'\n"),
        )
        .unwrap();
    }
    let scope = focus::Scope::from_paths(
        temp.path(),
        &[
            "crates/core/tests/area.rs".into(),
            "crates/core/tests/space name.rs".into(),
            "browser/tests/play.spec.ts".into(),
            "browser/tests/a[1].spec.ts".into(),
        ],
    )
    .unwrap();
    assert_eq!(scope.packages, ["aoe-core"]);
    assert_eq!(
        scope.specs,
        ["browser/tests/a[1].spec.ts", "browser/tests/play.spec.ts"]
    );
    let args = scope.cargo_args();
    assert!(args.windows(2).any(|a| a == ["-p", "aoe-core"]));
    assert!(!args.iter().any(|a| a == "--workspace" || a == "aoe-other"));
    assert_eq!(
        focus::spec_filter("tests/a[1].spec.ts"),
        r"(^|/)tests/a\[1\]\.spec\.ts$"
    );
    assert!(focus::Scope::from_paths(temp.path(), &["tests/unowned.rs".into()]).is_err());
}

#[test]
fn commit_subjects_are_bounded_by_characters_and_preserve_full_task_in_body() {
    let task = format!(
        "{}end\nAll task details\nMore details",
        "界 word ".repeat(20)
    );
    for phase in ["tests", "implementation"] {
        let message = tree::message(&task, phase, "codex", Some("model"));
        let subject = message.lines().next().unwrap();
        assert!(subject.chars().count() <= 72);
        assert!(subject.ends_with('…'));
        assert!(!subject.trim_end_matches('…').ends_with(' '));
        assert!(message.contains(&format!("\n\n{task}\n\n")));
        assert_eq!(subject.starts_with("Tests: "), phase == "tests");
    }
    let message = tree::message(&"x".repeat(200), "tests", "codex", None);
    assert_eq!(message.lines().next().unwrap(), "Tests: …");
    assert_eq!(
        tree::message("Short\nDetails", "implementation", "pi", None)
            .lines()
            .next(),
        Some("Short")
    );
}

#[test]
fn structured_test_results_preserve_names_and_unknown_failures_cannot_be_exempted() {
    let result = focus::rust_results(concat!(
        "{\"type\":\"test\",\"event\":\"ok\",\"name\":\"aoe-core::area$works\"}\n",
        "{\"type\":\"test\",\"event\":\"failed\",\"name\":\"aoe-core::area$broken\"}\n"
    ));
    assert_eq!(result.failed, ["aoe-core::area$broken"]);
    assert_eq!(
        result.tests,
        ["aoe-core::area$broken", "aoe-core::area$works"]
    );
    assert!(!focus::pre_existing(&result, &result));
    let mut untouched = result.clone();
    untouched.untouched = untouched.failed.clone();
    assert!(focus::pre_existing(&untouched, &result));
    let mut other = result.clone();
    other.failed = vec!["another failure".into()];
    assert!(!focus::pre_existing(&untouched, &other));
}

#[test]
fn untouched_detection_keeps_changed_tests_and_helpers_out_of_the_exemption() {
    use crate::ship::{
        git,
        tests::{fixture, run},
    };
    let (_temp, root) = fixture("true");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers=['crates/x']\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("crates/x/tests")).unwrap();
    fs::write(
        root.join("crates/x/Cargo.toml"),
        "[package]\nname='aoe-x'\n",
    )
    .unwrap();
    let original = "fn helper() {}\n#[test]\nfn existing() { helper(); }\n";
    fs::write(root.join("crates/x/tests/check.rs"), original).unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-qm", "existing tests"]);
    let base = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let scope = focus::Scope::from_paths(&root, &["crates/x/tests/check.rs".into()]).unwrap();
    fs::write(
        root.join("crates/x/tests/check.rs"),
        format!("{original}\n#[test]\nfn added() {{}}\n"),
    )
    .unwrap();
    let failures = [
        "aoe-x::check$existing".into(),
        "aoe-x::check$added".into(),
        "unknown".into(),
    ];
    assert_eq!(
        super::baseline::untouched(&root, &base, &scope, &failures).unwrap(),
        ["aoe-x::check$existing"]
    );
    fs::write(
        root.join("crates/x/tests/check.rs"),
        original.replace("helper();", "panic!();"),
    )
    .unwrap();
    assert!(
        super::baseline::untouched(&root, &base, &scope, &failures)
            .unwrap()
            .is_empty()
    );
    fs::write(
        root.join("crates/x/tests/check.rs"),
        original.replace("fn helper() {}", "fn helper() { panic!(); }"),
    )
    .unwrap();
    assert!(
        super::baseline::untouched(&root, &base, &scope, &failures)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn browser_results_include_each_project_and_ignore_line_numbers_in_failure_identity() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("report.json");
    fs::write(&path, r#"{"errors":[],"suites":[{"title":"play.spec.ts","file":"play.spec.ts","specs":[{"file":"play.spec.ts","line":10,"title":"works","ok":true,"tests":[{"projectName":"webgpu","status":"expected"},{"projectName":"canvas","status":"unexpected"}]}]}]}"#).unwrap();
    let report = focus::browser_results(&path).unwrap();
    assert_eq!(
        report.tests,
        [
            "play.spec.ts::works [webgpu]",
            "play.spec.ts::works [canvas]"
        ]
    );
    assert_eq!(report.failed, ["play.spec.ts::works [canvas]"]);
    assert_eq!(
        report.locations[&report.failed[0]],
        ("browser/tests/play.spec.ts".into(), 10)
    );
}

#[test]
fn browser_source_provenance_rejects_changed_assertions_helpers_and_unknown_syntax() {
    let original = "const value = 1;\ntest('old', async () => { expect(value).toBe(2); });\n";
    let added = format!("{original}test('new', async () => {{ expect(1).toBe(1); }});\n");
    assert!(super::specs::unchanged(&added, original, 2));
    assert!(!super::specs::unchanged(&added, original, 3));
    assert!(!super::specs::unchanged(
        &added.replace("toBe(2)", "toBe(3)"),
        original,
        2
    ));
    assert!(!super::specs::unchanged(
        &added.replace("value = 1", "value = 2"),
        original,
        2
    ));
    assert!(!super::specs::unchanged(
        "test(`dynamic`, () => {});",
        original,
        1
    ));
    assert!(!super::specs::unchanged(
        "test('old', () => { expect(x).toMatch(/x/); });",
        original,
        1
    ));
}

#[test]
fn baseline_uses_its_own_pre_duel_revision_without_changing_source_head_or_index() {
    use crate::ship::{git, tests::run};
    let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/tmp");
    fs::create_dir_all(&cache).unwrap();
    let owner = tempfile::Builder::new()
        .prefix("baseline-fixture-")
        .tempdir_in(cache)
        .unwrap();
    let root = owner.path().to_path_buf();
    run(&root, &["init", "-q"]);
    fs::write(root.join("value"), "before\n").unwrap();
    run(&root, &["add", "value"]);
    run(&root, &["commit", "-qm", "before"]);
    let base = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    fs::write(root.join("value"), "after\n").unwrap();
    run(&root, &["commit", "-qam", "after"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let index_path = git::git(&root, &["rev-parse", "--git-path", "index"]).unwrap();
    let index = fs::read(root.join(&index_path)).unwrap();
    let baseline = super::baseline::export(&root, &base).unwrap();
    assert_eq!(
        git::git(baseline.path(), &["rev-parse", "HEAD"]).unwrap(),
        base
    );
    assert_eq!(
        fs::read_to_string(baseline.path().join("value")).unwrap(),
        "before\n"
    );
    assert_eq!(git::git(&root, &["rev-parse", "HEAD"]).unwrap(), head);
    assert_eq!(fs::read(root.join(index_path)).unwrap(), index);
    assert!(!baseline.path().to_string_lossy().starts_with("/tmp/"));
}

#[test]
fn browser_provenance_handles_existing_templates_regex_and_computed_titles() {
    let old = "const title = `name ${1}`;\nconst pattern = /a[()]b/g;\ntest(title, async () => { expect(`x ${title}`).toMatch(pattern); });\n";
    let added = format!("{old}test('new', async () => {{ expect(1).toBe(1); }});\n");
    assert!(super::specs::unchanged(&added, old, 3));
    assert!(!super::specs::unchanged(
        &added.replace("a[()]b", "a[()]c"),
        old,
        3
    ));
    assert!(!super::specs::unchanged(
        &added.replace("toMatch(pattern)", "toMatch(/other/)"),
        old,
        3
    ));
}

#[test]
fn overridden_nextest_check_names_do_not_include_changing_progress_counters() {
    let red =
        super::runner::override_report("FAIL [ 0.1s] (2/10) aoe-core::coords old_test\n", false);
    let baseline =
        super::runner::override_report("FAIL [ 0.1s] (3/12) aoe-core::coords old_test\n", false);
    assert_eq!(red.failed, ["aoe-core::coords old_test"]);
    assert_eq!(red.failed, baseline.failed);
    assert_eq!(
        super::runner::override_report("FAIL plain_test\n", false).failed,
        ["plain_test"]
    );
}

#[test]
fn green_ignores_unrelated_tests_that_already_failed_at_red() {
    // `other.rs` is not a file the tester changed: its failure is pre-existing
    // and must not be required to pass at green.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("crates/x/tests")).unwrap();
    fs::write(
        root.join("crates/x/Cargo.toml"),
        "[package]\nname='aoe-x'\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/x/tests/other.rs"),
        "#[test]\nfn old() {}\n",
    )
    .unwrap();
    let paths = vec!["crates/x/tests/check.rs".into()];
    fs::write(root.join(&paths[0]), "#[test]\nfn first() {}\n").unwrap();
    let red = focus::CheckReport {
        failed: vec!["aoe-x::check$first".into(), "aoe-x::other$old".into()],
        untouched: vec!["aoe-x::other$old".into()],
        ..Default::default()
    };
    let green = focus::rust_results(
        "{\"type\":\"test\",\"event\":\"ok\",\"name\":\"aoe-x::check$first\"}\n{\"type\":\"test\",\"event\":\"failed\",\"name\":\"aoe-x::other$old\"}\n",
    );
    assert!(super::green::validate(root, &paths, None, &red, &green).is_ok());
    // The tester's own test still has to pass.
    let green = focus::rust_results(
        "{\"type\":\"test\",\"event\":\"failed\",\"name\":\"aoe-x::check$first\"}\n",
    );
    assert!(super::green::validate(root, &paths, None, &red, &green).is_err());
}
