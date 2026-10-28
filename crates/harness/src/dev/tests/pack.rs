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
