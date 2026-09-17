use aoe_assets::pack;
use std::{
    fs,
    path::{Path, PathBuf},
};

struct CurrentDirectory(PathBuf);

impl Drop for CurrentDirectory {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

fn fixture(command: &[u8], width: u32) -> Vec<u8> {
    let mut bytes = vec![0u8; 72];
    bytes[..4].copy_from_slice(b"2.0N");
    bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
    bytes[32..36].copy_from_slice(&68u32.to_le_bytes());
    bytes[36..40].copy_from_slice(&64u32.to_le_bytes());
    bytes[48..52].copy_from_slice(&width.to_le_bytes());
    bytes[52..56].copy_from_slice(&1u32.to_le_bytes());
    bytes[56..60].copy_from_slice(&1u32.to_le_bytes());
    bytes[68..72].copy_from_slice(&72u32.to_le_bytes());
    bytes.extend_from_slice(command);
    bytes
}

fn archive(sprite: &[u8], palette: &[u8]) -> Vec<u8> {
    let data = 112u32;
    let mut bytes = vec![0u8; data as usize];
    bytes[40..44].copy_from_slice(b"1.00");
    bytes[56..60].copy_from_slice(&2u32.to_le_bytes());
    bytes[60..64].copy_from_slice(&data.to_le_bytes());
    bytes[64..68].copy_from_slice(b" pls");
    bytes[68..72].copy_from_slice(&88u32.to_le_bytes());
    bytes[72..76].copy_from_slice(&1u32.to_le_bytes());
    bytes[76..80].copy_from_slice(b" pal");
    bytes[80..84].copy_from_slice(&100u32.to_le_bytes());
    bytes[84..88].copy_from_slice(&1u32.to_le_bytes());
    bytes[88..92].copy_from_slice(&100u32.to_le_bytes());
    bytes[92..96].copy_from_slice(&data.to_le_bytes());
    bytes[96..100].copy_from_slice(&(sprite.len() as u32).to_le_bytes());
    bytes[100..104].copy_from_slice(&50500u32.to_le_bytes());
    bytes[104..108].copy_from_slice(&(data + sprite.len() as u32).to_le_bytes());
    bytes[108..112].copy_from_slice(&(palette.len() as u32).to_le_bytes());
    bytes.extend_from_slice(sprite);
    bytes.extend_from_slice(palette);
    bytes
}

#[test]
fn imported_fixture_is_deterministic_and_verified() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(temporary.path()).expect("enter tempdir");
    let _restore = CurrentDirectory(original);
    fs::create_dir("trial").expect("trial directory");
    fs::write(
        "trial/colors.pal",
        b"JASC-PAL\n0100\n2\n12 34 56\n78 90 12\n",
    )
    .expect("palette");
    fs::write("trial/unit.slp", fixture(&[0x08, 0, 1, 0x0F], 2)).expect("sprite");
    fs::write("trial/readme.txt", b"unsupported").expect("other file");

    let inventory = pack::inspect(Path::new("trial")).expect("inspect");
    assert_eq!(inventory.slp, 1);
    assert_eq!(inventory.palettes, 1);
    assert_eq!(inventory.unsupported, vec!["readme.txt"]);

    let output = Path::new("local-assets/packs");
    let first = pack::import(Path::new("trial"), output).expect("import");
    let manifest = pack::verify(&first).expect("verify");
    assert_eq!(manifest.version, 1);
    assert_eq!(manifest.pages.len(), 1);
    assert_eq!(manifest.frames.len(), 1);
    assert_eq!(manifest.frames[0].source, "unit.slp");
    assert_eq!(manifest.frames[0].anchor_x, 1);
    assert_eq!(manifest.frames[0].width, 2);
    assert_eq!(
        pack::import(Path::new("trial"), output).expect("cache hit"),
        first
    );

    let manifest_path = first.join("manifest.json");
    let original_manifest = fs::read(&manifest_path).expect("manifest bytes");
    for (field, value) in [
        ("version", serde_json::json!(2)),
        ("input_hash", serde_json::json!("bad")),
    ] {
        let mut value_json: serde_json::Value =
            serde_json::from_slice(&original_manifest).expect("JSON");
        value_json[field] = value;
        fs::write(
            &manifest_path,
            serde_json::to_vec(&value_json).expect("encode"),
        )
        .expect("modify manifest");
        assert!(
            pack::verify(&first).is_err(),
            "field {field} must be rejected"
        );
    }
    fs::write(&manifest_path, &original_manifest).expect("restore manifest");
    for (pointer, value) in [
        ("/pages/0/color", serde_json::json!("../escape.png")),
        ("/frames/0/source_hash", serde_json::json!("invalid")),
        ("/frames/0/page", serde_json::json!(9)),
    ] {
        let mut value_json: serde_json::Value =
            serde_json::from_slice(&original_manifest).expect("JSON");
        *value_json.pointer_mut(pointer).expect("manifest field") = value;
        fs::write(
            &manifest_path,
            serde_json::to_vec(&value_json).expect("encode"),
        )
        .expect("modify manifest");
        assert!(
            pack::verify(&first).is_err(),
            "field {pointer} must be rejected"
        );
    }
    fs::write(&manifest_path, &original_manifest).expect("restore manifest");
    let mut duplicate: serde_json::Value =
        serde_json::from_slice(&original_manifest).expect("JSON");
    let duplicate_frame = duplicate["frames"][0].clone();
    duplicate["frames"]
        .as_array_mut()
        .expect("frames")
        .push(duplicate_frame);
    fs::write(
        &manifest_path,
        serde_json::to_vec(&duplicate).expect("encode"),
    )
    .expect("modify manifest");
    assert!(pack::verify(&first).is_err());
    fs::write(&manifest_path, &original_manifest).expect("restore manifest");
    let mut outside: serde_json::Value = serde_json::from_slice(&original_manifest).expect("JSON");
    outside["frames"][0]["x"] = serde_json::json!(2048);
    fs::write(
        &manifest_path,
        serde_json::to_vec(&outside).expect("encode"),
    )
    .expect("modify manifest");
    assert!(pack::verify(&first).is_err());
    fs::write(&manifest_path, &original_manifest).expect("restore manifest");
    assert!(pack::verify(&first).is_ok());

    let sprite = fixture(&[0x08, 0, 1, 0x0F], 2);
    let palette = b"JASC-PAL\n0100\n2\n12 34 56\n78 90 12\n";
    fs::create_dir("trial_drs").expect("DRS directory");
    fs::write("trial_drs/data.drs", archive(&sprite, palette)).expect("DRS archive");
    let drs_inventory = pack::inspect(Path::new("trial_drs")).expect("DRS inspect");
    assert_eq!(
        (drs_inventory.drs, drs_inventory.slp, drs_inventory.palettes),
        (1, 1, 1)
    );
    let drs_pack = pack::import(Path::new("trial_drs"), output).expect("DRS import");
    assert_eq!(pack::verify(&drs_pack).expect("DRS verify").frames.len(), 1);
    fs::write("trial_drs/duplicate.drs", archive(&sprite, palette)).expect("duplicate archive");
    assert!(pack::import(Path::new("trial_drs"), output).is_err());

    fs::remove_file("trial/colors.pal").expect("remove palette");
    assert!(pack::import(Path::new("trial"), output).is_err());
    assert!(pack::import(Path::new("trial"), Path::new("../outside")).is_err());
}
