use super::*;

#[test]
fn heavy_is_optional_and_defaults_to_light() {
    let mut value = proposal();
    gate_mut(&mut value, "fmt-check")
        .as_object_mut()
        .unwrap()
        .remove("heavy");
    let registry = parse(&value).expect("a gate without the field is light");
    assert!(
        !registry
            .gates
            .iter()
            .find(|g| g.id == "fmt-check")
            .unwrap()
            .heavy
    );
}

#[test]
fn the_committed_registry_marks_the_compiling_and_testing_gates_heavy() {
    // The whole set, so dropping any one flag fails here.
    let registry = registry();
    let heavy: BTreeSet<_> = registry
        .gates
        .iter()
        .filter(|gate| gate.heavy)
        .map(|gate| gate.id.as_str())
        .collect();
    let expected: BTreeSet<&str> = [
        "build-wasm",
        "coverage",
        "fuzz-nightly",
        "fuzz-smoke",
        "mutation-nightly",
        "perf-ci",
        "perf-full",
        "perf-instructions",
        "perf-pressure",
        "perf-smoke",
        "perf-soak-10",
        "perf-soak-30",
        "perf-stress",
        "perf-timing",
        "release-build",
        "test-e2e",
        "test-unit",
        "test-wasm",
    ]
    .into_iter()
    .collect();
    assert_eq!(heavy, expected);
    // Static gates run in the Stop hook: they must never queue for a slot,
    // or a busy machine would block a session from stopping.
    for gate in registry.gates.iter().filter(|gate| gate.r#static) {
        assert!(!gate.heavy, "{} is static and must not queue", gate.id);
    }
}

#[test]
fn heavy_must_be_a_boolean_and_unknown_fields_are_still_refused() {
    let mut value = proposal();
    gate_mut(&mut value, "lint")["heavy"] = json!("yes");
    assert!(parse(&value).is_err());
    let mut value = proposal();
    gate_mut(&mut value, "lint")["slots"] = json!(3);
    assert!(parse(&value).is_err());
}
