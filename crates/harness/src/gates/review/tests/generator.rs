use super::*;
#[test]
fn actual_catalog_registry_and_ast_changes_generate_new_closed_subjects() {
    let (owner, oid) = fixture();
    let root = owner.path();
    let (base, candidate) = snapshots(root, &oid, &oid);
    let original = materialize(&base, &candidate).unwrap();
    let catalog_path = root.join("gates/contracts.json");
    let mut catalog: Value = serde_json::from_slice(&fs::read(&catalog_path).unwrap()).unwrap();
    catalog["requirements"][0]["statement"] =
        json!("Changed declarative fixture contract, not execution evidence.");
    fs::write(&catalog_path, serde_json::to_vec(&catalog).unwrap()).unwrap();
    let catalog_oid = commit(root, Some(&oid));
    let (base, candidate) = snapshots(root, &oid, &catalog_oid);
    let changed = materialize(&base, &candidate).unwrap();
    assert_ne!(original.contracts["digest"], changed.contracts["digest"]);
    assert_eq!(changed.affected_paths, vec!["gates/contracts.json"]);
    assert!(assessment(&changed, Some(&review(&original))).is_err());
    let registry_path = root.join("gates/registry.json");
    let mut registry: Value = serde_json::from_slice(&fs::read(&registry_path).unwrap()).unwrap();
    registry["suites"][0]["paths"]
        .as_array_mut()
        .unwrap()
        .push(json!("docs/review-fixture/*.md"));
    fs::write(&registry_path, serde_json::to_vec(&registry).unwrap()).unwrap();
    let registry_oid = commit(root, Some(&catalog_oid));
    let (base, candidate) = snapshots(root, &oid, &registry_oid);
    let registry_changed = materialize(&base, &candidate).unwrap();
    assert_ne!(
        changed.canonical_registry_hash,
        registry_changed.canonical_registry_hash
    );
    assert!(assessment(&registry_changed, Some(&review(&changed))).is_err());
    let source = root.join("crates/demo/src/lib.rs");
    let bytes = fs::read_to_string(&source).unwrap();
    fs::write(&source, format!("// changed actual binding spans\n{bytes}")).unwrap();
    let source_oid = commit(root, Some(&registry_oid));
    let (base, candidate) = snapshots(root, &oid, &source_oid);
    let source_changed = materialize(&base, &candidate).unwrap();
    assert_ne!(
        registry_changed.contracts["bindings"][0]["binding"]["line"],
        source_changed.contracts["bindings"][0]["binding"]["line"]
    );
    assert_ne!(
        registry_changed.contracts["bindings"][0]["binding"]["file_blake3"],
        source_changed.contracts["bindings"][0]["binding"]["file_blake3"]
    );
    assert!(assessment(&source_changed, Some(&review(&registry_changed))).is_err());
    let plan = Registry::load(root)
        .unwrap()
        .plan(Cadence::Preflight, &BTreeSet::from(["everything".into()]))
        .unwrap();
    assert_eq!(source_changed.mandatory_preflight_plan, plan);
}
