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
    let heavy: BTreeSet<_> = registry()
        .gates
        .iter()
        .filter(|gate| gate.heavy)
        .map(|gate| gate.id.clone())
        .collect();
    for id in [
        "lint",
        "test-unit",
        "test-wasm",
        "test-e2e",
        "coverage",
        "mutation-nightly",
        "perf-smoke",
    ] {
        assert!(
            heavy.contains(id),
            "{id} compiles or runs the workspace's tests"
        );
    }
    for id in [
        "fmt-check",
        "structure-check",
        "test-placement",
        "architecture-check",
        "docs-check",
        "deny",
        "browser-check",
    ] {
        assert!(!heavy.contains(id), "{id} is a cheap static gate");
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
