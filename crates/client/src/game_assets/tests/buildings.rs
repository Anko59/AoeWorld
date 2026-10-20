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
fn feudal_native_art_is_selected_and_partial_present_sources_are_refused() {
    for (slot, stem, count) in [
        (26, "archery_range", 1),
        (27, "stable", 90),
        (28, "blacksmith", 1),
        (44, "dock", 1),
        (29, "market", 1),
        (30, "tower", 1),
        (39, "house", 3),
        (41, "barracks", 1),
    ] {
        let source = format!("de/b_west_{stem}_age2_x1.sld");
        let manifest = manifest_with(&source, count);
        let ranges = buildings::select(&manifest, &mut Vec::new(), &mut Vec::new()).unwrap();
        assert_eq!(ranges[slot], Some(0..if slot == 39 { 3 } else { 1 }));
    }
    let mut bad = manifest_with("de/b_west_archery_range_age2_x1.sld", 1);
    bad.frames[0].frame = 1;
    assert!(buildings::select(&bad, &mut Vec::new(), &mut Vec::new()).is_err());
}

#[wasm_bindgen_test]
fn trial_feudal_art_is_used_without_an_overlay_and_native_mill_parts_stay_separate() {
    for (slot, id, count) in [
        (26, 24, 1),
        (27, 1009, 1),
        (29, 2278, 1),
        (30, 2655, 1),
        (39, 2235, 3),
        (41, 133, 1),
        (44, 388, 1),
    ] {
        let manifest = manifest_with(&format!("graphics.drs:[32, 112, 108, 115]:{id}"), count);
        let ranges = buildings::select(&manifest, &mut Vec::new(), &mut Vec::new()).unwrap();
        assert!(ranges[slot].is_some());
    }
    let mut manifest = manifest_with("graphics.drs:[32, 112, 108, 115]:741", 1);
    manifest
        .frames
        .extend(manifest_with("graphics.drs:[32, 112, 108, 115]:737", 8).frames);
    let ranges = buildings::select(&manifest, &mut Vec::new(), &mut Vec::new()).unwrap();
    assert_eq!(ranges[40], Some(0..9));
    let bad = manifest_with("de/b_west_mill_age2_x1.sld", 90);
    assert!(buildings::select(&bad, &mut Vec::new(), &mut Vec::new()).is_err());
}

#[wasm_bindgen_test]
fn building_selection_preserves_trial_fallback_and_rejects_incomplete_builder_rows() {
    let manifest = manifest_with("graphics.drs:[32, 112, 108, 115]:2223", 3);
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    let groups = buildings::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(groups[1], Some(0..3));
    assert_eq!(selected, vec![0, 1, 2]);
    assert!(domains.iter().all(|d| *d == AtlasDomain::Objects));
    let bad = manifest_with("graphics.drs:[32, 112, 108, 115]:1496", 74);
    assert!(buildings::select(&bad, &mut Vec::new(), &mut Vec::new()).is_err());
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
    let groups = buildings::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(groups[4], Some(0..44));
    assert_eq!(selected, (0..44).collect::<Vec<_>>());
    assert!(domains.iter().all(|d| *d == AtlasDomain::Objects));
    manifest.frames.remove(43);
    assert!(buildings::select(&manifest, &mut Vec::new(), &mut Vec::new()).is_err());
}

#[wasm_bindgen_test]
fn farm_uses_a_bounded_texture_window_without_hiding_invalid_source_bounds() {
    let mut manifest = manifest_with("de/g_fm1.dds", 1);
    manifest.frames[0].width = 1024;
    manifest.frames[0].height = 1024;
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    let groups = buildings::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(domains, vec![AtlasDomain::Terrain]);
    buildings::crop_farm(&mut manifest, &selected, &groups).unwrap();
    assert_eq!(
        (manifest.frames[0].width, manifest.frames[0].height),
        (256, 256)
    );
    manifest.frames[0].x = 1900;
    manifest.frames[0].width = 1024;
    assert!(buildings::crop_farm(&mut manifest, &selected, &groups).is_err());
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

#[wasm_bindgen_test]
fn layered_feudal_mill_avoids_repeating_the_body_in_the_fixed_object_atlas() {
    let mut manifest = manifest_with("de/b_west_mill_age2_x1.sld", 91);
    for frame in &mut manifest.frames {
        frame.width = 320;
        frame.height = 280;
    }
    manifest
        .frames
        .extend(manifest_with("graphics.drs:[32, 112, 108, 115]:741", 1).frames);
    manifest
        .frames
        .extend(manifest_with("graphics.drs:[32, 112, 108, 115]:737", 8).frames);
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    let groups = buildings::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(groups[40], Some(0..9));
    assert_eq!(selected, (91..100).collect::<Vec<_>>());
    placement::plan(&manifest, &selected, &domains).unwrap();
    // A present DE source remains validated even when the trial parts win.
    manifest.frames.remove(90);
    assert!(buildings::select(&manifest, &mut Vec::new(), &mut Vec::new()).is_err());
}

#[wasm_bindgen_test]
fn feudal_texture_sampling_preserves_native_geometry_and_source_bounds() {
    let mut manifest = manifest_with("de/b_west_blacksmith_age2_x1.sld", 1);
    let frame = &mut manifest.frames[0];
    frame.width = 5;
    frame.height = 3;
    frame.anchor_x = 4;
    frame.anchor_y = 2;
    let placed = placement::plan(&manifest, &[0], &[AtlasDomain::Objects]).unwrap()[0];
    assert_eq!((placed.width, placed.height), (3, 2));
    let native = placement::record(&manifest.frames[0], placed);
    assert_eq!(native.size, [5., 3.]);
    assert_eq!(native.anchor, [4., 2.]);
    let mut source = vec![0; GAME_ATLAS_PAGE_BYTES];
    for row in 0..3 {
        for col in 0..5 {
            let at = (row * GAME_ATLAS_SIDE as usize + col) * 4;
            source[at..at + 4].copy_from_slice(&[row as u8, col as u8, 71, 255]);
        }
    }
    let mut pixels = vec![0; GAME_ATLAS_BYTES];
    placement::copy_plane(
        &manifest.frames[0],
        placed,
        placement::Plane::Color,
        &source,
        &mut pixels,
        [0; 3],
    )
    .unwrap();
    for row in 0..2 {
        for col in 0..3 {
            let at = ((usize::from(placed.y) + row) * GAME_ATLAS_SIDE as usize
                + usize::from(placed.x)
                + col)
                * 4
                + usize::from(placed.page) * GAME_ATLAS_PAGE_BYTES;
            assert_eq!(
                &pixels[at..at + 4],
                &[(row * 2) as u8, (col * 2) as u8, 71, 255]
            );
        }
    }
    manifest.frames[0].x = 2046;
    assert!(placement::plan(&manifest, &[0], &[AtlasDomain::Objects]).is_err());
    let dark = manifest_with("de/b_dark_house_age1_x1.sld", 3);
    let p = placement::plan(&dark, &[0], &[AtlasDomain::Objects]).unwrap()[0];
    assert_eq!(
        (p.width, p.height),
        (dark.frames[0].width, dark.frames[0].height)
    );
}
