use super::*;

#[test]
fn transitive_only_job_membership_cannot_hide_a_selected_gate() {
    let mut value = proposal();
    value["jobs"]["target-performance"] = json!(["perf-ci", "perf-pressure", "perf-wasm-size"]);
    // perf-ci still depends on perf-instructions, but direct selection must
    // nevertheless identify the job that executes the selected prerequisite.
    assert!(
        parse(&value)
            .unwrap_err()
            .to_string()
            .contains("no job coverage")
    );
}

#[test]
fn independent_prerequisite_selects_its_executing_job_without_broadening_suites() {
    let mut value = proposal();
    gate_mut(&mut value, "perf-instructions")["suites"] = json!(["static"]);
    let registry = parse(&value).expect("valid covered gate");
    let impact = registry.classify(&paths(&["docs/testing.md"]));
    let plan = registry.plan(Cadence::Ci, &impact.suites).expect("CI plan");
    assert_eq!(plan.suites, suite_set(&["docs", "static"]));
    assert!(plan.gates.contains(&"perf-instructions".into()));
    assert!(plan.jobs["target-performance"]);
    assert!(!plan.jobs["native-coverage"]);
}

#[test]
fn registry_evidence_change_invalidates_same_revision_selection() {
    use crate::gates::{check_selection, selection};
    let original = registry();
    let mut changed = original.clone();
    changed.gates[0].evidence.push_str(" reviewed change");
    let paths = paths(&["docs/testing.md"]);
    let old = selection(&original, "a".repeat(40), None, paths.clone()).unwrap();
    let new = selection(&changed, "a".repeat(40), None, paths).unwrap();
    assert_eq!(old.gates, new.gates);
    assert_ne!(old.registry_hash, new.registry_hash);
    let mut results = BTreeMap::from([("select".to_owned(), "success")]);
    results.extend(
        old.jobs
            .iter()
            .map(|(job, selected)| (job.clone(), if *selected { "success" } else { "skipped" })),
    );
    let results = serde_json::to_string(&results).unwrap();
    let manifest = serde_json::to_string(&old).unwrap();
    check_selection(&manifest, &results, &old).expect("passing control");
    assert!(check_selection(&manifest, &results, &new).is_err());
}
