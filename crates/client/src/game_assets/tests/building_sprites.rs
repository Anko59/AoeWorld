use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn manifest_with(source: &str, count: u32) -> Manifest {
    let mut value = manifest::fixture();
    let frame = value["frames"][0].clone();
    value["frames"] = serde_json::Value::Array(
        (0..count)
            .map(|i| {
                let mut f = frame.clone();
                f["source"] = source.into();
                f["frame"] = i.into();
                f
            })
            .collect(),
    );
    serde_json::from_value(value).unwrap()
}

#[wasm_bindgen_test]
fn building_selection_preserves_trial_fallback_and_rejects_incomplete_builder_rows() {
    let manifest = manifest_with("graphics.drs:[32, 112, 108, 115]:2223", 3);
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    let groups = building_sprites::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(groups[1], Some(0..3));
    assert_eq!(selected, vec![0, 1, 2]);
    assert!(domains.iter().all(|d| *d == AtlasDomain::Objects));
    let bad = manifest_with("graphics.drs:[32, 112, 108, 115]:1496", 74);
    assert!(building_sprites::select(&bad, &mut Vec::new(), &mut Vec::new()).is_err());
}

#[wasm_bindgen_test]
fn layered_trial_mill_keeps_its_body_and_all_native_sail_frames() {
    let mut manifest = manifest_with("graphics.drs:[32, 112, 108, 115]:3483", 1);
    manifest
        .frames
        .extend(manifest_with("graphics.drs:[32, 112, 108, 115]:3482", 43).frames);
    manifest
        .frames
        .extend(manifest_with("de/b_dark_mill_age1_x1.sld", 181).frames);
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    let groups = building_sprites::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(groups[4], Some(0..44));
    assert_eq!(selected, (0..44).collect::<Vec<_>>());
    assert!(domains.iter().all(|d| *d == AtlasDomain::Objects));
    manifest.frames.remove(43);
    assert!(building_sprites::select(&manifest, &mut Vec::new(), &mut Vec::new()).is_err());
}

#[wasm_bindgen_test]
fn farm_uses_a_bounded_texture_window_without_hiding_invalid_source_bounds() {
    let mut manifest = manifest_with("de/g_fm1.dds", 1);
    manifest.frames[0].width = 1024;
    manifest.frames[0].height = 1024;
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    let groups = building_sprites::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(domains, vec![AtlasDomain::Terrain]);
    building_sprites::crop_farm(&mut manifest, &selected, &groups).unwrap();
    assert_eq!(
        (manifest.frames[0].width, manifest.frames[0].height),
        (256, 256)
    );
    manifest.frames[0].x = 1900;
    manifest.frames[0].width = 1024;
    assert!(building_sprites::crop_farm(&mut manifest, &selected, &groups).is_err());
}

#[wasm_bindgen_test]
fn standing_compaction_keeps_exactly_the_native_frame_already_drawn_per_facing() {
    let manifest = manifest_with("graphics.drs:[32, 112, 108, 115]:3004", 50);
    let frames = source_frames(&manifest, "graphics.drs:[32, 112, 108, 115]:3004", 50).unwrap();
    assert_eq!(
        compact_standing(frames, AssetRole::CavalryStanding),
        vec![0, 10, 20, 30, 40]
    );
    assert_eq!(
        compact_standing((0..75).collect(), AssetRole::VillagerStanding),
        vec![0, 15, 30, 45, 60]
    );
    assert_eq!(
        compact_standing((0..50).collect(), AssetRole::CavalryWalking),
        (0..50).collect::<Vec<_>>()
    );
}
