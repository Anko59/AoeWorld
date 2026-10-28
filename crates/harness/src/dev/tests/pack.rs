use super::*;

#[test]
fn a_pack_the_importer_would_reject_is_refused_with_the_regeneration_step() {
    let pack = tempfile::tempdir().unwrap();
    std::fs::write(
        pack.path().join("manifest.json"),
        br#"{"version":1,"frames":[]}"#,
    )
    .unwrap();
    let message = verify_asset_pack(pack.path()).unwrap_err().to_string();
    assert!(message.contains("make assets-import"), "{message}");
    assert!(message.contains("not valid for this build"), "{message}");
}

#[test]
fn a_pack_with_building_recipes_but_no_valid_overlay_is_refused() {
    let pack = tempfile::tempdir().unwrap();
    // Packs without building recipes need no overlay.
    require_building_overlay(pack.path(), false).unwrap();
    let missing = require_building_overlay(pack.path(), true)
        .unwrap_err()
        .to_string();
    assert!(
        missing.contains("no attached Definitive Edition"),
        "{missing}"
    );
    assert!(missing.contains("make assets-import-de"), "{missing}");
    let overlay = pack.path().join("buildings");
    std::fs::create_dir(&overlay).unwrap();
    std::fs::write(
        overlay.join("manifest.json"),
        br#"{"version":1,"frames":[]}"#,
    )
    .unwrap();
    let invalid = require_building_overlay(pack.path(), true)
        .unwrap_err()
        .to_string();
    assert!(invalid.contains("invalid building overlay"), "{invalid}");
}
