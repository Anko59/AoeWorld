use super::{
    authored,
    focus::{CheckReport, CompileError, Panic},
    panic_only_refusal, runner, wasm,
};
use crate::{process::CaptureExit, wasm_test::SuiteRun};
use std::collections::BTreeMap;

/// Every field non-empty. Written without `..Default::default()`, so adding a field to
/// `CheckReport` fails to compile here until it is filled and `merge_combines_every_field` covers it.
fn full(tag: &str) -> CheckReport {
    CheckReport {
        passed: false,
        build_failed: tag == "a",
        unavailable: (tag == "b").then(|| "b unavailable".to_owned()),
        tests: vec![format!("{tag}$test")],
        failed: vec![format!("{tag}$failed")],
        untouched: vec![format!("{tag}$untouched")],
        locations: BTreeMap::from([(format!("{tag}$failed"), (format!("{tag}.rs"), 1))]),
        panics: vec![Panic {
            path: format!("{tag}.rs"),
            location: format!("{tag}.rs:1:1"),
            message: format!("{tag} boom"),
            test: Some(format!("{tag}$failed")),
        }],
        compile_errors: vec![CompileError {
            path: format!("{tag}.rs"),
            code: Some("E0425".into()),
            items: vec![format!("{tag}_item")],
        }],
    }
}

#[test]
fn merge_combines_every_field() {
    let mut merged = CheckReport {
        passed: true,
        ..Default::default()
    };
    merged.merge(full("a"));
    merged.merge(full("b"));
    assert!(!merged.passed && merged.build_failed);
    assert_eq!(merged.unavailable.as_deref(), Some("b unavailable"));
    assert_eq!(merged.tests, ["a$test", "b$test"]);
    assert_eq!(merged.failed, ["a$failed", "b$failed"]);
    assert_eq!(merged.untouched, ["a$untouched", "b$untouched"]);
    assert_eq!(merged.locations.len(), 2);
    let locations: Vec<_> = merged.panics.iter().map(|p| p.location.as_str()).collect();
    assert_eq!(locations, ["a.rs:1:1", "b.rs:1:1"]);
    let items: Vec<_> = merged
        .compile_errors
        .iter()
        .map(|e| e.items[0].as_str())
        .collect();
    assert_eq!(items, ["a_item", "b_item"]);
    // The first unavailable reason stands, and a passing report keeps a passing one passing.
    let mut first = CheckReport {
        passed: true,
        unavailable: Some("first".into()),
        ..Default::default()
    };
    first.merge(full("b"));
    assert_eq!(first.unavailable.as_deref(), Some("first"));
    let mut ok = CheckReport {
        passed: true,
        ..Default::default()
    };
    ok.merge(CheckReport {
        passed: true,
        ..Default::default()
    });
    assert!(ok.passed);
    // The text a failed check shows carries the names and each panic's place and assertion.
    let text = merged.failure_text();
    assert!(text.starts_with("a$failed, b$failed"), "{text}");
    assert!(text.contains("panicked at b.rs:1:1\nb boom"), "{text}");
    // Several tests panicking at one place show it once.
    let mut twice = full("a");
    twice.merge(full("a"));
    assert_eq!(twice.failure_text().matches("panicked at").count(), 1);
}

const FIXTURE: &str = "crates/rendering/src/surface_mesh/mix/tests/fixture.rs";

fn run(exit: CaptureExit, output: &str) -> CheckReport {
    wasm::results(&[SuiteRun {
        package: "aoe-rendering",
        binary: "lib",
        exit,
        output: output.into(),
    }])
    .unwrap()
}

const PANIC: &str = "panicked at crates/rendering/src/surface_mesh/mix/tests/fixture.rs:311:5:\nassertion failed: x\n";

#[test]
fn a_panic_is_attributed_to_the_failing_test_it_follows() {
    // A named failure with its panic after the FAIL line, and one under a `---- name output ----` header.
    for output in [
        format!("test surface_mesh::a ... FAIL\n{PANIC}"),
        format!("failures:\n\n---- surface_mesh::a output ----\n{PANIC}"),
    ] {
        let report = run(CaptureExit::Failed(Some(1)), &output);
        let panic = &report.panics[0];
        assert_eq!(panic.test.as_deref(), Some("aoe-rendering$surface_mesh::a"));
    }
    // After a passing test's line, or alone, a panic names no test.
    for output in [
        format!("test surface_mesh::a ... ok\n{PANIC}"),
        PANIC.to_owned(),
    ] {
        let report = run(CaptureExit::Failed(Some(1)), &output);
        assert_eq!(report.panics[0].test, None);
    }
}

#[test]
fn only_a_panic_no_pre_existing_test_explains_is_red_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let changed = vec![FIXTURE.to_owned()];
    let judge = |report: &CheckReport| authored::evidence(temp.path(), &changed, report).unwrap().0;
    let a = "aoe-rendering$surface_mesh::a";
    // A pre-existing failing test whose assertion panics inside a tester-changed helper: not authored.
    let mut report = run(
        CaptureExit::Failed(Some(1)),
        &format!("test surface_mesh::a ... FAIL\n{PANIC}"),
    );
    report.untouched = vec![a.into()];
    assert!(!judge(&report));
    // The same panic printed before its FAIL line has no owner, but the only failure is pre-existing.
    let mut report = run(
        CaptureExit::Failed(Some(1)),
        &format!("{PANIC}test surface_mesh::a ... FAIL\n"),
    );
    report.untouched = vec![a.into()];
    assert!(!judge(&report));
    // A nameless panic in a tester-changed file is authored.
    assert!(judge(&run(CaptureExit::Failed(Some(1)), PANIC)));
    // A named tester test (not untouched) with its panic is authored, as before.
    let report = run(
        CaptureExit::Failed(Some(1)),
        &format!("test surface_mesh::a ... FAIL\n{PANIC}"),
    );
    assert!(judge(&report));
}

#[test]
fn an_attack_round_drops_a_panic_only_red_where_round_one_refuses_it() {
    let report = run(CaptureExit::Failed(Some(1)), PANIC);
    let refusal = panic_only_refusal(1, &report, false).unwrap();
    assert!(refusal.contains("fixture.rs:311:5"), "{refusal}");
    assert!(panic_only_refusal(2, &report, false).is_none());
    assert!(panic_only_refusal(1, &report, true).is_none());
    let aborted = run(
        CaptureExit::Failed(Some(1)),
        "test a ... ok\ntest exited abnormally\n",
    );
    assert!(
        panic_only_refusal(1, &aborted, false)
            .unwrap()
            .contains("without a named test")
    );
    assert!(panic_only_refusal(3, &aborted, false).is_none());
    assert!(runner::names(&report).contains("panicked at"));
}
