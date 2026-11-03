use super::*;

#[test]
fn de_icon_inventory_handles_native_case_and_rejects_ambiguous_indices() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("012_50730.DDS"), []).unwrap();
    fs::write(root.path().join("103_50730.dds"), []).unwrap();
    fs::write(root.path().join("012_other.dds"), []).unwrap();
    let files = inventory(root.path()).unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[&12].file_name().unwrap(), "012_50730.DDS");
    fs::write(root.path().join("12_50730.dds"), []).unwrap();
    assert!(inventory(root.path()).is_err());
}

#[test]
fn de_icon_catalog_uses_only_the_exact_unit_sheet_cell() {
    let hash = "a".repeat(64);
    let manifest: crate::pack::Manifest = serde_json::from_value(serde_json::json!({
        "version":1, "converter":"synthetic", "input_hash":hash,
        "pages":[{"color":"color-000.png", "color_hash":hash,
            "player":"player-000.png", "player_hash":hash,
            "shadow":"shadow-000.png", "shadow_hash":hash,
            "outline":"outline-000.png", "outline_hash":hash,
            "width":2048,"height":2048}],
        "frames":[{"source":"de/interface/12.dds", "source_hash":hash,
            "frame":0,"page":0,"x":1,"y":2,"width":256,"height":256,
            "anchor_x":0,"anchor_y":0,"native":null}],
        "chunks":[],"terrains":[],"buildings":[],"building_data":"00"
    }))
    .unwrap();
    let icons = catalog(&manifest, &[(473, 12)], &[]).unwrap();
    let art = &icons.units[&473];
    assert_eq!(
        (&art.page, art.x, art.y, art.width),
        (&"color-000.png".to_owned(), 1, 2, 256)
    );
    assert!(catalog(&manifest, &[(567, 72)], &[]).is_err());
    let mut both = manifest.clone();
    let mut technology = both.frames[0].clone();
    technology.source = "de/interface/tech/12.dds".into();
    technology.x = 300;
    both.frames.push(technology);
    let icons = catalog(&both, &[(473, 12)], &[(103, 12)]).unwrap();
    assert_eq!(icons.units[&473].x, 1);
    assert_eq!(icons.technologies[&103].x, 300);
    assert!(catalog(&manifest, &[], &[(103, 12)]).is_err());
    let mut invalid = manifest;
    invalid.frames[0].page = 10;
    assert!(catalog(&invalid, &[(473, 12)], &[]).is_err());
}

#[test]
fn de_tech_inventory_uses_numeric_prefix_and_rejects_ambiguous_indices() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("032_synthetic_age.DDS"), []).unwrap();
    fs::write(root.path().join("not-an-index.dds"), []).unwrap();
    fs::write(root.path().join("033_synthetic.txt"), []).unwrap();
    let files = named_inventory(root.path(), None).unwrap();
    assert_eq!(files.len(), 1);
    assert!(files.contains_key(&32));
    fs::write(root.path().join("32_duplicate.dds"), []).unwrap();
    assert!(named_inventory(root.path(), None).is_err());
}

#[test]
fn de_icon_dimensions_are_bounded_before_texture_decode() {
    let mut bytes = vec![0; 128];
    bytes[12..16].copy_from_slice(&4096_u32.to_le_bytes());
    bytes[16..20].copy_from_slice(&4096_u32.to_le_bytes());
    let error = decode(&bytes).err().expect("oversized icon must fail");
    assert!(error.to_string().contains("icon dimensions"));
}
