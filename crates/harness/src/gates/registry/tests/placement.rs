use super::*;

#[test]
fn docs_do_not_select_code_via_static_flag_or_dependency_broadening() {
    let registry = registry();
    let impact = registry.classify(&paths(&["docs/testing.md", "README.md"]));
    assert_eq!(impact.suites, suite_set(&["docs", "static"]));
    let plan = registry.plan(Cadence::Pr, &impact.suites).unwrap();
    assert_eq!(
        plan.gates,
        paths(&[
            "docs-check",
            "fmt-check",
            "structure-check",
            "test-placement"
        ])
    );
    assert!(plan.jobs["static"]);
    for name in [
        "native-coverage",
        "browser",
        "target-performance",
        "fuzz-smoke",
    ] {
        assert!(!plan.jobs[name]);
    }
    assert!(!plan.gates.contains(&"release-source-check".into()));
}
