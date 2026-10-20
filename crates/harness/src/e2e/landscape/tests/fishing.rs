use super::*;

#[test]
fn synthetic_coast_has_an_inland_village_dock_site_and_both_fish_kinds() {
    let root = tempfile::tempdir().unwrap();
    let store = root.path().join("maps");
    let hash = prepare(root.path(), &store).unwrap();
    super::super::verify_store(&store).unwrap();
    let scene: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.path().join(".cache/fishing/scene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["hash"], hash);
    assert!(scene["deep"]["x"].as_i64().unwrap() > scene["dock"]["x"].as_i64().unwrap());
    assert_eq!(prepare(root.path(), &store).unwrap(), hash);
}
