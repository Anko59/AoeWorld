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
fn disk_hygiene_red_cleans_its_private_run_on_every_outcome() {
    for outcome in [
        "success",
        "already green",
        "runner error",
        "green failure",
        "panic",
    ] {
        let (temp, base, _) = history();
        let root = temp.path();
        let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
        let mut snapshots = Vec::new();
        let mut calls = 0;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            red::run_with(root, &base, &head, &mut |snapshot, _| {
                calls += 1;
                snapshots.push(snapshot.to_owned());
                let owner = snapshot.parent().unwrap();
                assert_ne!(owner, root.join(".cache/agent-hook/red"));
                let build = snapshot.join("target/debug/incremental");
                fs::create_dir_all(&build).unwrap();
                fs::write(build.join("object"), "build output").unwrap();
                // Logs are retained outside the run owner, even on failure.
                let logs = root.join(".cache/agent-hook/red/output");
                fs::create_dir_all(&logs).unwrap();
                fs::write(logs.join("bounded.log"), "runner log").unwrap();
                match outcome {
                    "runner error" => Err("runner unavailable".into()),
                    "panic" => panic!("runner panic"),
                    "already green" => Ok(result(true)),
                    "green failure" => Ok(result(false)),
                    _ => Ok(result(calls == 2)),
                }
            })
        }));
        match outcome {
            "success" => assert!(result.unwrap().is_ok()),
            "panic" => assert!(result.is_err()),
            _ => assert!(result.unwrap().is_err()),
        }
        for snapshot in snapshots {
            assert!(!snapshot.exists(), "{outcome}: {}", snapshot.display());
            assert!(!snapshot.parent().unwrap().exists(), "run owner removed");
        }
        assert!(
            root.join(".cache/agent-hook/red/output/bounded.log")
                .is_file()
        );
        assert_eq!(
            red::require(root, &base, &head).is_ok(),
            outcome == "success"
        );
    }
}

#[test]
fn disk_hygiene_overlapping_reds_leave_the_active_runs_builds_alone() {
    let (temp, base, _) = history();
    let root = temp.path();
    let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    let mut calls = 0;
    red::run_with(root, &base, &head, &mut |outer, _| {
        calls += 1;
        let build = outer.join("target/active");
        fs::create_dir_all(build.parent().unwrap()).unwrap();
        fs::write(&build, "active").unwrap();
        let mut inner_calls = 0;
        red::run_with(root, &base, &head, &mut |inner, _| {
            assert_ne!(inner.parent(), outer.parent(), "each run owns its exports");
            inner_calls += 1;
            assert!(build.is_file());
            Ok(result(inner_calls == 2))
        })?;
        assert!(
            build.is_file(),
            "finishing another red must not touch this run"
        );
        Ok(result(calls == 2))
    })
    .unwrap();
}

#[test]
fn browser_red_runs_parent_with_tests_then_head_and_records_exact_commits() {
    let (temp, base, _) = history();
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
    assert_eq!(record.paths, [SPEC]);
    assert_eq!(record.blobs.len(), 1);
    assert_eq!(record.failed, [NAME]);
    assert!(record.missing.is_empty());
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
    let blocking = crate::ship::Options {
        red_blocking: true,
        ..crate::ship::tests::offline()
    };
    let error = crate::ship::ship(&root, &blocking).unwrap_err().to_string();
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
fn skipped_red_tests_and_unavailable_runs_are_refused() {
    for failure in ["skipped", "unavailable"] {
        let (temp, base, _) = history();
        let root = temp.path();
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
    let (temp, _) = repo();
    let root = temp.path();
    let path = "crates/sim/src/tests.rs";
    commit(
        root,
        &[
            (path, "// no tests yet\n"),
            (
                "crates/sim/src/lib.rs",
                "pub fn f() {}\n#[cfg(test)] mod tests;\n",
            ),
        ],
        "fixture mounts test module",
    );
    let base = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    commit(root, &[(path, "#[test]\nfn red_case() {}\n")], "tests");
    commit(
        root,
        &[(
            "crates/sim/src/lib.rs",
            "pub fn f() { 1; }\n#[cfg(test)] mod tests;\n",
        )],
        "implementation",
    );
    let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    let mut calls = 0;
    let record = red::run_with(root, &base, &head, &mut |snapshot, _| {
        calls += 1;
        let has_test = fs::read_to_string(snapshot.join(path))
            .unwrap()
            .contains("red_case");
        let green = !has_test
            || fs::read_to_string(snapshot.join("crates/sim/src/lib.rs"))
                .unwrap()
                .contains("1;");
        Ok(CheckReport {
            passed: green,
            tests: if has_test {
                vec!["sim$tests::red_case".into()]
            } else {
                vec![]
            },
            failed: if !green {
                vec!["sim$tests::red_case".into()]
            } else {
                vec![]
            },
            ..Default::default()
        })
    })
    .unwrap();
    assert_eq!(calls, 3, "red, old base tests, then green must each run");
    assert_eq!(record.failed, ["sim$tests::red_case"]);
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
