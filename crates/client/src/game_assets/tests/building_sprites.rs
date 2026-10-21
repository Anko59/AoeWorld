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
    let groups = building_sprites::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(groups[40], Some(0..9));
    assert_eq!(selected, (91..100).collect::<Vec<_>>());
    placement::plan(&manifest, &selected, &domains).unwrap();
    // A present DE source remains validated even when the trial parts win.
    manifest.frames.remove(90);
    assert!(building_sprites::select(&manifest, &mut Vec::new(), &mut Vec::new()).is_err());
}

#[wasm_bindgen_test]
fn feudal_textures_pack_every_source_pixel_with_native_geometry_and_source_bounds() {
    let mut manifest = manifest_with("de/b_west_blacksmith_age2_x1.sld", 1);
    let frame = &mut manifest.frames[0];
    frame.width = 5;
    frame.height = 3;
    frame.anchor_x = 4;
    frame.anchor_y = 2;
    let placed = placement::plan(&manifest, &[0], &[AtlasDomain::Objects]).unwrap()[0];
    assert_eq!((placed.width, placed.height), (5, 3));
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
    for row in 0..3 {
        for col in 0..5 {
            let at = ((usize::from(placed.y) + row) * GAME_ATLAS_SIDE as usize
                + usize::from(placed.x)
                + col)
                * 4
                + usize::from(placed.page) * GAME_ATLAS_PAGE_BYTES;
            assert_eq!(&pixels[at..at + 4], &[row as u8, col as u8, 71, 255]);
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

#[wasm_bindgen_test]
fn native_ambient_and_construction_texels_keep_native_geometry() {
    for source in [
        "graphics.drs:[32, 112, 108, 115]:4518",
        "graphics.drs:[32, 112, 108, 115]:4397",
        "graphics.drs:[32, 112, 108, 115]:733",
        "graphics.drs:[32, 112, 108, 115]:4616",
    ] {
        let mut manifest = manifest_with(source, 1);
        let f = &mut manifest.frames[0];
        f.width = 5;
        f.height = 3;
        f.anchor_x = -4;
        f.anchor_y = 8;
        let placed = placement::plan(&manifest, &[0], &[AtlasDomain::Objects]).unwrap()[0];
        assert_eq!((placed.width, placed.height), (5, 3));
        let record = placement::record(&manifest.frames[0], placed);
        assert_eq!(record.size, [5.0, 3.0]);
        assert_eq!(record.anchor, [-4.0, 8.0]);
    }
}
