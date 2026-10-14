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
fn no_committed_gate_is_heavy_until_the_flags_are_set() {
    // The flags land in their own change: the judge built from dev must first
    // know the field before gates/registry.json may carry it.
    assert!(registry().gates.iter().all(|gate| !gate.heavy));
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
