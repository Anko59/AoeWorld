use super::{commit, repo, write};
use crate::ship::{duel::focus::CheckReport, git, test_first::red};
use std::{fs, path::Path};

const SPEC: &str = "browser/tests/red.spec.ts";
const NAME: &str = "red.spec.ts::page has its description [webgpu]";

fn history() -> (tempfile::TempDir, String, String) {
    let (temp, base) = repo();
    commit(
        temp.path(),
        &[(SPEC, "test('page has its description', () => {});\n")],
        "tests",
    );
    let tests = git::git(temp.path(), &["rev-parse", "HEAD"]).unwrap();
    commit(
        temp.path(),
        &[("web/index.html", "description\n")],
        "implementation",
    );
    (temp, base, tests)
}

fn result(green: bool) -> CheckReport {
    CheckReport {
        passed: green,
        tests: vec![NAME.into()],
        failed: if green { vec![] } else { vec![NAME.into()] },
        locations: [(NAME.into(), (SPEC.into(), 1))].into(),
        ..Default::default()
    }
}

#[test]
fn browser_red_runs_parent_with_tests_then_head_and_records_exact_commits() {
    let (temp, base, tests) = history();
    let root = temp.path();
    let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    let mut calls = 0;
    let record = red::run_with(root, &base, &head, &mut |snapshot, paths| {
        assert_eq!(paths, &[SPEC]);
        assert!(snapshot.join(SPEC).is_file());
        calls += 1;
        assert_eq!(snapshot.join("web/index.html").exists(), calls == 2);
        Ok(result(calls == 2))
    })
    .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(record.head, head);
    assert_eq!(record.base, base);
    assert_eq!(record.proofs[0].tests, tests);
    assert_eq!(record.proofs[0].parent, base);
    assert_eq!(record.proofs[0].failed, [NAME]);
    assert!(red::require(root, &base, &head).unwrap().contains(NAME));
    commit(root, &[("README.md", "later\n")], "later");
    let later = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        red::require(root, &base, &later)
            .unwrap_err()
            .contains("make red")
    );
}

#[test]
fn browser_that_already_passes_or_never_turns_green_cannot_write_a_record() {
    for already_passes in [true, false] {
        let (temp, base, _) = history();
        let root = temp.path();
        let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
        let error =
            red::run_with(root, &base, &head, &mut |_, _| Ok(result(already_passes))).unwrap_err();
        assert!(
            error.contains(if already_passes {
                "already pass"
            } else {
                "green"
            }),
            "{error}"
        );
        assert!(red::require(root, &base, &head).is_err());
    }
}

#[test]
fn writable_forged_evidence_is_ignored_and_ship_refuses_product_without_red() {
    let (_temp, root) = crate::ship::tests::fixture("true");
    commit(&root, &[("web/index.html", "product\n")], "implementation");
    let base = git::git(&root, &["rev-parse", "origin/dev"]).unwrap();
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    for path in [
        ".cache/duel/check.json",
        ".cache/red/record.json",
        "reports/red.json",
    ] {
        write(
            &root,
            path,
            &format!("{{\"head\":\"{head}\",\"base\":\"{base}\",\"passed\":true}}"),
        );
    }
    // Keep fake artifacts ignored, as they are in the real checkout.
    fs::write(root.join(".git/info/exclude"), ".cache/\nreports/\n").unwrap();
    let error = crate::ship::ship(&root, &crate::ship::tests::offline())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("red record") && error.contains("make red"),
        "{error}"
    );
}

#[test]
fn declared_refactor_exemption_allows_ship_and_is_reported() {
    let (_temp, root) = crate::ship::tests::fixture("true");
    commit(
        &root,
        &[("web/index.html", "product\n")],
        "refactor\n\nHarness-Test-First: exempt — pure refactor: reorder markup",
    );
    let evidence = crate::ship::ship(&root, &crate::ship::tests::offline()).unwrap();
    let text = red::require(&root, &evidence.merge_base, &evidence.head).unwrap();
    assert!(
        text.contains("exempt") && text.contains("pure refactor"),
        "{text}"
    );
}

#[test]
fn changed_or_skipped_red_tests_and_unavailable_runs_are_refused() {
    for failure in ["changed", "skipped", "unavailable"] {
        let (temp, base, _) = history();
        let root = temp.path();
        if failure == "changed" {
            commit(
                root,
                &[(SPEC, "test.skip('page has its description', () => {});\n")],
                "change test",
            );
        }
        let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
        let mut calls = 0;
        let outcome = red::run_with(root, &base, &head, &mut |_: &Path, _| {
            calls += 1;
            let mut report = result(calls == 2);
            if failure == "skipped" && calls == 2 {
                report.tests.clear();
            }
            if failure == "unavailable" {
                report.unavailable = Some("timeout".into());
            }
            Ok(report)
        });
        assert!(outcome.is_err(), "{failure}: {outcome:?}");
    }
}

#[test]
fn rust_named_failure_uses_the_same_exact_commit_record() {
    let (temp, base) = repo();
    let root = temp.path();
    let path = "crates/sim/src/tests.rs";
    commit(root, &[(path, "#[test]\nfn red_case() {}\n")], "tests");
    commit(
        root,
        &[("crates/sim/src/lib.rs", "pub fn f() { 1; }\n")],
        "implementation",
    );
    let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    let mut calls = 0;
    let record = red::run_with(root, &base, &head, &mut |_, _| {
        calls += 1;
        Ok(CheckReport {
            passed: calls == 2,
            tests: vec!["sim$tests::red_case".into()],
            failed: if calls == 1 {
                vec!["sim$tests::red_case".into()]
            } else {
                vec![]
            },
            ..Default::default()
        })
    })
    .unwrap();
    assert_eq!(record.proofs[0].failed, ["sim$tests::red_case"]);
    assert!(red::require(root, &base, &head).is_ok());
}

#[test]
fn an_unrelated_failure_is_not_red_evidence() {
    let (temp, base, _) = history();
    let root = temp.path();
    let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    let error = red::run_with(root, &base, &head, &mut |_, _| {
        let mut report = result(false);
        report.locations.get_mut(NAME).unwrap().0 = "browser/tests/untouched.spec.ts".into();
        Ok(report)
    })
    .unwrap_err();
    assert!(error.contains("no changed spec/test failed"), "{error}");
}
