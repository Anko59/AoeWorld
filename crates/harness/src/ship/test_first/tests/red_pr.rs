//! The proof is of the whole PR, whatever its commit layout; only the runner is injected.
use super::{commit, repo};
use crate::ship::{
    duel::focus::{CheckReport, CompileError},
    git,
    test_first::red,
};
use std::fs;

const TESTS: &str = "crates/sim/src/tests.rs";
const LIB: &str = "pub fn f() {}\n#[cfg(test)] mod tests;\n";
const NAME: &str = "sim$tests::feature";

/// A base whose crate mounts a test module, and the branch point after it.
fn mounted() -> (tempfile::TempDir, String) {
    let (temp, _) = repo();
    commit(
        temp.path(),
        &[(TESTS, "// none\n"), ("crates/sim/src/lib.rs", LIB)],
        "mount tests",
    );
    let base = git::git(temp.path(), &["rev-parse", "HEAD"]).unwrap();
    (temp, base)
}

fn tip(root: &std::path::Path) -> String {
    git::git(root, &["rev-parse", "HEAD"]).unwrap()
}

/// The test fails unless the product tree has `fixed`.
fn runner(snapshot: &std::path::Path) -> CheckReport {
    let has_test = fs::read_to_string(snapshot.join(TESTS)).is_ok_and(|s| s.contains("feature"));
    let fixed = fs::read_to_string(snapshot.join("crates/sim/src/lib.rs"))
        .unwrap()
        .contains("fixed");
    CheckReport {
        passed: !has_test || fixed,
        tests: if has_test { vec![NAME.into()] } else { vec![] },
        failed: if has_test && !fixed {
            vec![NAME.into()]
        } else {
            vec![]
        },
        ..Default::default()
    }
}

#[test]
fn one_commit_mixing_product_and_tests_is_proven() {
    let (temp, base) = mounted();
    let root = temp.path();
    commit(
        root,
        &[
            (
                TESTS,
                "#[test]\nfn feature() { assert!(super::fixed()); }\n",
            ),
            (
                "crates/sim/src/lib.rs",
                "pub fn fixed() -> bool { true }\n#[cfg(test)] mod tests;\n",
            ),
        ],
        "tests and product together",
    );
    let head = tip(root);
    let mut calls = Vec::new();
    let record = red::run_with(root, &base, &head, &mut |snapshot, _| {
        calls.push(fs::read_to_string(snapshot.join("crates/sim/src/lib.rs")).unwrap());
        Ok(runner(snapshot))
    })
    .unwrap();
    assert!(
        !calls[0].contains("fixed"),
        "red runs the product as before the PR"
    );
    assert!(!calls[1].contains("fixed"), "old tests run at the base");
    assert!(
        calls.last().unwrap().contains("fixed"),
        "green runs the PR head"
    );
    assert_eq!(record.failed, [NAME]);
    assert!(red::require(root, &base, &head).unwrap().contains(NAME));
}

#[test]
fn a_changed_test_that_passes_at_the_merge_base_is_refused() {
    let (temp, base) = mounted();
    let root = temp.path();
    commit(
        root,
        &[
            (TESTS, "#[test]\nfn feature() {}\n"),
            (
                "crates/sim/src/lib.rs",
                "pub fn fixed() {}\n#[cfg(test)] mod tests;\n",
            ),
        ],
        "mixed",
    );
    let head = tip(root);
    let error = red::run_with(root, &base, &head, &mut |_, _| {
        Ok(CheckReport {
            passed: true,
            tests: vec![NAME.into()],
            ..Default::default()
        })
    })
    .unwrap_err();
    assert!(error.contains("already pass"), "{error}");
    assert!(red::require(root, &base, &head).is_err());
}

#[test]
fn a_failure_present_at_the_merge_base_never_counts() {
    let (temp, base) = mounted();
    let root = temp.path();
    // The base already has the test, failing; the PR rewrites its body and fixes nothing.
    commit(
        root,
        &[(TESTS, "#[test]\nfn feature() { old(); }\n")],
        "existing failing test",
    );
    let base = {
        let _ = base;
        tip(root)
    };
    commit(
        root,
        &[
            (TESTS, "#[test]\nfn feature() { old(); more(); }\n"),
            (
                "crates/sim/src/lib.rs",
                "pub fn fixed() {}\n#[cfg(test)] mod tests;\n",
            ),
        ],
        "mixed",
    );
    let head = tip(root);
    let error = red::run_with(root, &base, &head, &mut |snapshot, _| {
        let fixed = fs::read_to_string(snapshot.join("crates/sim/src/lib.rs"))
            .unwrap()
            .contains("fixed");
        Ok(CheckReport {
            passed: fixed,
            tests: vec![NAME.into()],
            failed: if fixed { vec![] } else { vec![NAME.into()] },
            ..Default::default()
        })
    })
    .unwrap_err();
    assert!(error.contains("already failed"), "{error}");
}

fn compile_failure(path: &str, code: Option<&str>, items: &[&str]) -> CheckReport {
    CheckReport {
        passed: false,
        build_failed: true,
        failed: vec!["package build failed: sim".into()],
        compile_errors: vec![CompileError {
            path: path.into(),
            code: code.map(str::to_owned),
            items: items.iter().map(|i| (*i).to_owned()).collect(),
        }],
        ..Default::default()
    }
}

/// A test calling `new_api`, which the PR adds to the product crate.
fn new_api(adds_it: bool) -> (tempfile::TempDir, String, String) {
    let (temp, base) = mounted();
    let root = temp.path();
    commit(
        root,
        &[
            (
                TESTS,
                "#[test]\nfn feature() { assert!(super::new_api()); }\n",
            ),
            (
                "crates/sim/src/lib.rs",
                if adds_it {
                    "pub fn new_api() -> bool { true }\n#[cfg(test)] mod tests;\n"
                } else {
                    "pub fn other() {}\n#[cfg(test)] mod tests;\n"
                },
            ),
        ],
        "new api and its test",
    );
    let head = tip(root);
    (temp, base, head)
}

fn green(snapshot: &std::path::Path) -> CheckReport {
    let built = fs::read_to_string(snapshot.join("crates/sim/src/lib.rs"))
        .unwrap()
        .contains("new_api");
    let calls = fs::read_to_string(snapshot.join(TESTS))
        .unwrap()
        .contains("new_api");
    if built || !calls {
        CheckReport {
            passed: true,
            tests: vec![NAME.into()],
            ..Default::default()
        }
    } else {
        compile_failure(TESTS, Some("E0425"), &["new_api"])
    }
}

#[test]
fn a_test_against_an_api_the_pr_adds_is_compile_red_and_names_it() {
    let (temp, base, head) = new_api(true);
    let root = temp.path();
    let record = red::run_with(root, &base, &head, &mut |snapshot, paths| {
        assert_eq!(paths, [TESTS]);
        Ok(green(snapshot))
    });
    // The base run (old tests, no new_api use) compiles; only the red run does not.
    let record = record.unwrap_or_else(|e| panic!("{e}"));
    assert!(record.failed.is_empty());
    assert_eq!(record.missing, ["new_api"]);
    assert_eq!(record.ran, [NAME]);
    let text = red::require(root, &base, &head).unwrap();
    assert!(text.contains("missing: new_api"), "{text}");
}

#[test]
fn a_compile_red_whose_tests_do_not_run_at_head_is_refused() {
    let (temp, base, head) = new_api(true);
    let root = temp.path();
    // Filtered, ignored or cfg-gated: head is green but names no test of the file, or only a
    // test of another file, or a failing one.
    for tests in [vec![], vec!["sim$other::elsewhere".to_owned()]] {
        let error = red::run_with(root, &base, &head, &mut |snapshot, _| {
            Ok(CheckReport {
                tests: tests.clone(),
                ..green(snapshot)
            })
        })
        .unwrap_err();
        assert!(error.contains("ran and passed at HEAD"), "{error}");
        assert!(red::require(root, &base, &head).is_err());
    }
    let error = red::run_with(root, &base, &head, &mut |snapshot, _| {
        let mut report = green(snapshot);
        if report.passed {
            report.passed = false;
            report.failed = vec![NAME.into()];
        }
        Ok(report)
    })
    .unwrap_err();
    assert!(error.contains("green"), "{error}");
}

#[test]
fn a_compile_red_record_without_ran_tests_does_not_satisfy_ship() {
    let (temp, base, head) = new_api(true);
    let root = temp.path();
    red::run_with(root, &base, &head, &mut |snapshot, _| Ok(green(snapshot))).unwrap();
    assert!(red::require(root, &base, &head).unwrap().contains(NAME));
    let path = git::git(root, &["rev-parse", "--git-path", "aoe-ship/red"]).unwrap();
    let file = root.join(path).join(format!("{head}.json"));
    let mut record: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    record["ran"] = serde_json::json!([]);
    fs::write(&file, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(red::require(root, &base, &head).is_err());
}

#[test]
fn a_compile_error_the_pr_does_not_explain_is_not_red() {
    let (temp, base, head) = new_api(false);
    let root = temp.path();
    for report in [
        compile_failure(TESTS, Some("E0425"), &["new_api"]),
        compile_failure("crates/sim/src/lib.rs", Some("E0425"), &["other"]),
        compile_failure(TESTS, Some("E0308"), &["other"]),
        compile_failure(TESTS, None, &["other"]),
    ] {
        let outcome = red::run_with(root, &base, &head, &mut |snapshot, _| {
            Ok(
                if snapshot.join(TESTS).is_file()
                    && fs::read_to_string(snapshot.join(TESTS))
                        .unwrap()
                        .contains("feature")
                {
                    report.clone()
                } else {
                    CheckReport {
                        passed: true,
                        tests: vec![NAME.into()],
                        ..Default::default()
                    }
                },
            )
        });
        let error = outcome.unwrap_err();
        assert!(
            error.contains("not a changed test calling an API the PR adds"),
            "{error}"
        );
    }
}

#[test]
fn an_edit_after_the_proof_or_another_base_voids_the_record() {
    let (temp, base) = mounted();
    let root = temp.path();
    commit(
        root,
        &[
            (TESTS, "#[test]\nfn feature() {}\n"),
            (
                "crates/sim/src/lib.rs",
                "pub fn fixed() {}\n#[cfg(test)] mod tests;\n",
            ),
        ],
        "mixed",
    );
    let head = tip(root);
    red::run_with(root, &base, &head, &mut |snapshot, _| Ok(runner(snapshot))).unwrap();
    assert!(red::require(root, &base, &head).is_ok());
    assert!(
        red::require(
            root,
            &git::git(root, &["rev-parse", &format!("{base}~1")]).unwrap(),
            &head
        )
        .is_err()
    );
    commit(
        root,
        &[(TESTS, "#[test]\nfn feature() { changed(); }\n")],
        "edit test",
    );
    assert!(
        red::require(root, &base, &tip(root)).is_err(),
        "a new head needs its own proof"
    );
}
