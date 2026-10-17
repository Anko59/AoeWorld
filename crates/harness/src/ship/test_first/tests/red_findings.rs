//! Review regressions use real histories; only the selected check is injected.
use super::{commit, repo};
use crate::ship::{duel::focus::CheckReport, git, test_first::red};
use std::fs;

const SPEC: &str = "browser/tests/smoke.spec.ts";
const OLD: &str = "test('old', () => { check(); });\n";
const BOTH: &str = "test('old', () => { check(); });\ntest('new', () => {});\n";

fn browser(names: &[&str], failed: &[&str]) -> CheckReport {
    let json = serde_json::json!({"suites": [{"title": "smoke.spec.ts", "file": "smoke.spec.ts",
        "specs": names.iter().enumerate().map(|(i, n)| serde_json::json!({
            "title": n, "file": "smoke.spec.ts", "line": i + 1,
            "tests": [{"projectName": "webgpu", "status": if failed.contains(n) { "unexpected" } else { "expected" }}]
        })).collect::<Vec<_>>()
    }], "errors": []});
    let file = tempfile::NamedTempFile::new().unwrap();
    fs::write(file.path(), serde_json::to_vec(&json).unwrap()).unwrap();
    let mut report = crate::ship::duel::focus::browser_results(file.path()).unwrap();
    report.passed = failed.is_empty();
    report
}

#[test]
fn red_refuses_unchanged_browser_failure_beside_new_passing_test() {
    let (temp, _) = repo();
    let root = temp.path();
    commit(root, &[(SPEC, OLD)], "existing failing test");
    let base = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    commit(root, &[(SPEC, BOTH)], "tests: new already passes");
    commit(root, &[("web/index.html", "fix old\n")], "fix");
    let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    let outcome = red::run_with(root, &base, &head, &mut |snapshot, _| {
        Ok(browser(
            &["old", "new"],
            if snapshot.join("web/index.html").exists() {
                &[]
            } else {
                &["old"]
            },
        ))
    });
    assert!(
        outcome.is_err(),
        "unchanged failure earned proof: {outcome:?}"
    );
    assert!(red::require(root, &base, &head).is_err());
}

#[test]
fn red_refuses_changed_test_that_already_failed_with_old_test_bytes() {
    let (temp, _) = repo();
    let root = temp.path();
    commit(root, &[(SPEC, OLD)], "existing failure");
    let base = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    commit(
        root,
        &[(SPEC, "test('old', () => { check(); extra(); });\n")],
        "tests",
    );
    commit(root, &[("web/index.html", "fix\n")], "fix");
    let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    let mut saw_old = false;
    let outcome = red::run_with(root, &base, &head, &mut |snapshot, _| {
        saw_old |= fs::read_to_string(snapshot.join(SPEC)).unwrap() == OLD;
        Ok(browser(
            &["old"],
            if snapshot.join("web/index.html").exists() {
                &[]
            } else {
                &["old"]
            },
        ))
    });
    assert!(
        outcome.is_err(),
        "pre-existing failure earned proof: {outcome:?}"
    );
    assert!(
        saw_old,
        "must run the tests commit's parent with OLD test files"
    );
}

#[test]
fn red_attributes_helper_changes_to_importing_specs_only() {
    for kind in ["direct", "transitive", "unrelated", "comment"] {
        let imports = matches!(kind, "direct" | "transitive");
        let (temp, _) = repo();
        let root = temp.path();
        let helper = "browser/tests/surface-evidence.ts";
        let source = match kind {
            "direct" => {
                "import { check } from './surface-evidence';\ntest('old', () => { check(); });\n"
            }
            "transitive" => "import { check } from './middle';\ntest('old', () => { check(); });\n",
            "comment" => {
                "// import { check } from './surface-evidence';\ntest('old', () => { check(); });\n"
            }
            _ => OLD,
        };
        commit(
            root,
            &[
                (SPEC, source),
                (helper, "export function check() {}\n"),
                (
                    "browser/tests/middle.ts",
                    "export { check } from './surface-evidence';\n",
                ),
            ],
            "base",
        );
        let base = git::git(root, &["rev-parse", "HEAD"]).unwrap();
        commit(
            root,
            &[(helper, "export function check() { assertMetadata(); }\n")],
            "tests helper",
        );
        commit(root, &[("web/index.html", "metadata\n")], "implementation");
        let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
        let outcome = red::run_with(root, &base, &head, &mut |snapshot, _| {
            let green = snapshot.join("web/index.html").exists()
                || !fs::read_to_string(snapshot.join(helper))
                    .unwrap()
                    .contains("assertMetadata");
            let mut report = browser(&["old"], if green { &[] } else { &["old"] });
            report
                .locations
                .values_mut()
                .for_each(|(_, line)| *line = if kind == "unrelated" { 1 } else { 2 });
            Ok(report)
        });
        assert_eq!(outcome.is_ok(), imports, "{outcome:?}");
    }
}

#[test]
fn red_rust_attribution_uses_changed_function_not_file_or_leaf_name() {
    for changed_fails in [true, false] {
        let (temp, _) = repo();
        let root = temp.path();
        let path = "crates/sim/src/tests.rs";
        let old = "#[test]\nfn case() {}\nmod nested { #[test] fn case() {} }\n";
        commit(
            root,
            &[
                (path, old),
                (
                    "crates/sim/src/lib.rs",
                    "pub fn f() {}\n#[cfg(test)] mod tests;\n",
                ),
            ],
            "base tests",
        );
        let base = git::git(root, &["rev-parse", "HEAD"]).unwrap();
        commit(
            root,
            &[(
                path,
                "#[test]\nfn case() { new_assert(); }\nmod nested { #[test] fn case() {} }\n",
            )],
            "tests",
        );
        commit(root, &[("web/index.html", "fix\n")], "implementation");
        let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
        let failed = if changed_fails {
            "sim$tests::case"
        } else {
            "sim$tests::nested::case"
        };
        let mut saw_old = false;
        let outcome = red::run_with(root, &base, &head, &mut |snapshot, _| {
            let before = fs::read_to_string(snapshot.join(path)).unwrap() == old;
            saw_old |= before;
            let green = before || snapshot.join("web/index.html").exists();
            Ok(CheckReport {
                passed: green,
                tests: vec![failed.into()],
                failed: if green { vec![] } else { vec![failed.into()] },
                ..Default::default()
            })
        });
        assert_eq!(outcome.is_ok(), changed_fails, "{outcome:?}");
        if changed_fails {
            assert!(saw_old);
        }
    }
}

#[test]
fn red_still_refuses_nameless_or_unexplained_panics() {
    use crate::ship::duel::focus::Panic;
    for owner in [None, Some("another test")] {
        let (temp, base) = repo();
        let root = temp.path();
        commit(root, &[(SPEC, OLD)], "tests");
        commit(root, &[("web/index.html", "fix")], "implementation");
        let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
        let outcome = red::run_with(root, &base, &head, &mut |_, _| {
            let mut report = browser(&["old"], &["old"]);
            report.panics.push(Panic {
                path: SPEC.into(),
                location: format!("{SPEC}:1:1"),
                message: "abort".into(),
                test: owner.map(str::to_owned),
            });
            Ok(report)
        });
        assert!(
            outcome.is_err(),
            "nameless or unexplained panic earned proof: {outcome:?}"
        );
        assert!(red::require(root, &base, &head).is_err());
    }
}

#[test]
fn red_does_not_attribute_a_running_test_to_an_unmounted_rust_file() {
    let (temp, _) = repo();
    let root = temp.path();
    commit(
        root,
        &[
            (
                "crates/sim/src/lib.rs",
                "#[cfg(test)]\n#[path = \"real_tests.rs\"] mod tests;\n",
            ),
            (
                "crates/sim/src/real_tests.rs",
                "#[test]\nfn case() { assert!(false); }\n",
            ),
        ],
        "existing mounted test",
    );
    let base = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    commit(
        root,
        &[(
            "crates/sim/src/tests.rs",
            "#[test]\nfn case() { assert!(true); }\n",
        )],
        "unmounted passing test",
    );
    commit(root, &[("web/index.html", "implementation")], "fix");
    let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    let outcome = red::run_with(root, &base, &head, &mut |snapshot, _| {
        let green = snapshot.join("web/index.html").exists();
        Ok(CheckReport {
            passed: green,
            tests: vec!["sim$tests::case".into()],
            failed: if green {
                vec![]
            } else {
                vec!["sim$tests::case".into()]
            },
            ..Default::default()
        })
    });
    assert!(
        outcome.is_err(),
        "dormant file laundered another test's failure: {outcome:?}"
    );
    assert!(red::require(root, &base, &head).is_err());
}
