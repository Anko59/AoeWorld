use super::*;

#[test]
fn long_is_optional_and_must_be_boolean() {
    let registry = parse(&proposal()).unwrap();
    assert!(
        !registry
            .gates
            .iter()
            .find(|g| g.id == "fmt-check")
            .unwrap()
            .long
    );
    let mut value = proposal();
    gate_mut(&mut value, "lint")["long"] = json!("yes");
    assert!(parse(&value).is_err());
}

#[test]
fn removed_slot_configuration_is_not_a_registry_field() {
    let mut value = proposal();
    gate_mut(&mut value, "lint")["heavy"] = json!(true);
    assert!(parse(&value).is_err());
}

#[test]
fn long_marking_is_accepted_and_retained_by_the_parser() {
    let mut value = proposal();
    gate_mut(&mut value, "lint")["long"] = json!(true);
    let registry = parse(&value).expect("a registry naming a long job parses");
    assert!(registry.gates.iter().find(|gate| gate.id == "lint").unwrap().long);
}
