use super::*;
use serde_json::{Value, json};
fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn catalog() -> Value {
    serde_json::from_slice(&fs::read(root().join("gates/contracts.json")).unwrap()).unwrap()
}
fn parse(value: &Value) -> Result<schema::Catalog> {
    schema::Catalog::parse(
        &serde_json::to_vec(value).unwrap(),
        &Registry::load(&root()).unwrap(),
    )
}
#[test]
fn checked_real_catalog_binds_reachable_production_and_tests_without_claiming_execution() {
    let report = check(&root()).unwrap();
    assert_eq!(report["status"], "STRUCTURAL_REFERENCES_VALID");
    assert_eq!(report["authoritative"], false);
    assert_eq!(report["semantic_test_evidence"], "NOT_ASSESSED");
    assert_eq!(report["coverage_claim"], "NOT_ASSESSED");
    assert_eq!(report["requirements"], 3);
    assert!(report["bindings"].as_array().unwrap().len() >= 9);
    assert!(
        report["deferred"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["status"] == "UNAVAILABLE")
    );
}
#[test]
fn unknown_fields_schema_bounds_and_nonprintable_empty_text_fail_closed() {
    for pointer in [
        "",
        "/requirements/0",
        "/requirements/0/invariants/0",
        "/requirements/0/invariants/0/boundaries/0",
        "/cases/0",
        "/deferred/0",
    ] {
        let mut value = catalog();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("budget_s".into(), json!(0));
        assert!(parse(&value).is_err());
    }
    for value in [json!(0), json!(2)] {
        let mut c = catalog();
        c["schema"] = value;
        assert!(parse(&c).is_err());
    }
    for value in [
        json!(""),
        json!("  "),
        json!("invalid\ntext"),
        json!("x".repeat(1025)),
    ] {
        let mut c = catalog();
        c["requirements"][0]["statement"] = value;
        assert!(parse(&c).is_err());
    }
    let registry = Registry::load(&root()).unwrap();
    assert!(schema::Catalog::parse(&vec![b' '; MAX_BYTES + 1], &registry).is_err());
    for field in ["requirements", "cases"] {
        let mut c = catalog();
        c[field] = json!([]);
        assert!(parse(&c).is_err());
    }
    for (field, count) in [("requirements", 65), ("cases", 257), ("deferred", 33)] {
        let mut c = catalog();
        let item = c[field][0].clone();
        c[field] = json!(vec![item; count]);
        assert!(parse(&c).is_err());
    }
    for value in ["INVALID", "-invalid", "bad id", ""] {
        let mut c = catalog();
        c["requirements"][0]["id"] = json!(value);
        assert!(parse(&c).is_err());
    }
    let mut c = catalog();
    c["requirements"][0]["risk"] = json!("approved");
    assert!(parse(&c).is_err());
    let mut c = catalog();
    c["deferred"][0]["capability"] = json!("qualified");
    assert!(parse(&c).is_err());
}
#[test]
fn dangling_unused_duplicate_and_misclassified_case_references_fail() {
    let mut c = catalog();
    c["cases"][1]["id"] = c["cases"][0]["id"].clone();
    assert!(parse(&c).is_err());
    let mut c = catalog();
    c["cases"][1]["path"] = c["cases"][0]["path"].clone();
    c["cases"][1]["symbol"] = c["cases"][0]["symbol"].clone();
    assert!(parse(&c).is_err());
    let mut c = catalog();
    c["requirements"][0]["invariants"][0]["case_ids"] = json!(["missing-case"]);
    assert!(parse(&c).is_err());
    let mut c = catalog();
    let mut unused = c["cases"][0].clone();
    unused["id"] = json!("unused-case");
    unused["symbol"] = json!("unused_symbol");
    c["cases"].as_array_mut().unwrap().push(unused);
    assert!(parse(&c).is_err());
    for (field, value) in [
        ("gate", json!("browser")),
        ("runner", json!("wasm-test")),
        ("kind", json!("qualified")),
        ("path", json!("crates/../untracked.rs")),
        ("symbol", json!("invalid::")),
    ] {
        let mut c = catalog();
        c["cases"][0][field] = value;
        assert!(parse(&c).is_err());
    }
    let mut c = catalog();
    c["requirements"][0]["gate_refs"] = json!(["test-unit", "architecture-check"]);
    assert!(parse(&c).is_err());
    let mut c = catalog();
    for item in c["cases"].as_array_mut().unwrap() {
        item["kind"] = json!("positive");
    }
    assert!(parse(&c).is_err());
    let mut c = catalog();
    c["requirements"][0]["invariants"][0]["boundaries"] = json!([]);
    assert!(parse(&c).is_err());
}
#[test]
fn missing_and_renamed_real_items_are_not_replaced_by_catalog_assertions() {
    let mut c = catalog();
    c["cases"][0]["symbol"] = json!("tests::nonexistent_contract_case");
    let parsed = parse(&c).unwrap();
    let case = &parsed.cases[0];
    assert!(ast::bind(&root(), &case.path, &case.symbol, "test").is_err());
    let parsed = parse(&catalog()).unwrap();
    let boundary = &parsed.requirements[0].invariants[0].boundaries[0];
    assert!(
        ast::bind(
            &root(),
            &boundary.path,
            "nonexistent_boundary",
            boundary.kind.name()
        )
        .is_err()
    );
    let mut registry = Registry::load(&root()).unwrap();
    registry.gates.retain(|gate| gate.id != "test-unit");
    assert!(schema::Catalog::parse(&serde_json::to_vec(&catalog()).unwrap(), &registry).is_err());
}
#[test]
fn catalog_file_bounds_and_linked_paths_are_rejected_before_binding() {
    let owner = tempfile::tempdir().unwrap();
    fs::create_dir(owner.path().join("gates")).unwrap();
    let path = owner.path().join("gates/contracts.json");
    fs::write(&path, vec![b'x'; MAX_BYTES + 1]).unwrap();
    assert!(catalog_bytes(owner.path()).is_err());
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("gates")).unwrap();
    fs::create_dir(dir.path().join("gates/contracts.json")).unwrap();
    assert!(catalog_bytes(dir.path()).is_err());
    #[cfg(unix)]
    {
        let linked = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(owner.path().join("gates"), linked.path().join("gates"))
            .unwrap();
        assert!(catalog_bytes(linked.path()).is_err());
    }
}
