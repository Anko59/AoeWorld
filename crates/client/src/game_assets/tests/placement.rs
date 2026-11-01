use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn fixture() -> Manifest {
    serde_json::from_value(manifest::fixture()).unwrap()
}

#[path = "placement/native.rs"]
mod native;

#[wasm_bindgen_test]
fn count_and_source_bounds_are_checked_before_destination_allocation() {
    let manifest = fixture();
    let selected = vec![0; MAX_SELECTED_FRAMES];
    let domains = vec![AtlasDomain::Terrain; MAX_SELECTED_FRAMES];
    assert_eq!(
        plan(&manifest, &selected, &domains).unwrap().len(),
        MAX_SELECTED_FRAMES
    );
    assert!(
        plan(
            &manifest,
            &vec![0; MAX_SELECTED_FRAMES + 1],
            &vec![AtlasDomain::Terrain; MAX_SELECTED_FRAMES + 1]
        )
        .is_err()
    );
    assert!(plan(&manifest, &[1], &[AtlasDomain::Objects]).is_err());
    let mut manifest = manifest;
    manifest.frames[0].page = u16::MAX;
    assert!(plan(&manifest, &[0], &[AtlasDomain::Objects]).is_err());
    manifest.frames[0].page = 0;
    manifest.frames[0].x = 2047;
    assert!(plan(&manifest, &[0], &[AtlasDomain::Objects]).is_err());
    manifest.frames[0].x = 0;
    manifest.frames[0].width = 2045;
    assert!(plan(&manifest, &[0], &[AtlasDomain::Objects]).is_err());
    manifest.frames[0].width = 0;
    assert!(plan(&manifest, &[0], &[AtlasDomain::Objects]).is_err());
    manifest.frames[0].width = 2;
    manifest.pages[0].height = 2047;
    assert!(plan(&manifest, &[0], &[AtlasDomain::Objects]).is_err());
}

#[wasm_bindgen_test]
fn placements_preserve_semantic_order_domains_and_signed_anchors() {
    let mut value = manifest::fixture();
    let first = value["frames"][0].clone();
    value["frames"] = serde_json::json!([first.clone(), first.clone(), first]);
    for (index, width) in [5, 9, 10].into_iter().enumerate() {
        value["frames"][index]["width"] = width.into();
        value["frames"][index]["height"] = 5.into();
        value["frames"][index]["anchor_x"] = (-(index as i32) - 7).into();
    }
    let manifest: Manifest = serde_json::from_value(value).unwrap();
    let selected = [2, 1, 0];
    let places = plan(
        &manifest,
        &selected,
        &[
            AtlasDomain::Terrain,
            AtlasDomain::Objects,
            AtlasDomain::Terrain,
        ],
    )
    .unwrap();
    assert_eq!(places.iter().map(|p| p.page).collect::<Vec<_>>(), [0, 2, 0]);
    assert_eq!(
        places.iter().map(|p| p.width).collect::<Vec<_>>(),
        [10, 9, 5]
    );
    for (semantic, &source) in selected.iter().enumerate() {
        let frame = record(&manifest.frames[source], places[semantic]);
        assert_eq!(frame.anchor[0], -(source as f32) - 7.0);
        assert_eq!(frame.atlas.page, u32::from(places[semantic].page));
        assert_eq!(frame.size[0], manifest.frames[source].width as f32);
    }
    assert_eq!(
        renderer_topology(TerrainFrameTopology::CoordinateStableAccents),
        TerrainTopology::CoordinateStableAccents
    );
    assert_eq!(
        renderer_topology(TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 10,
            rows: 10
        }),
        TerrainTopology::PeriodicXMajorReversedY {
            columns: 10,
            rows: 10
        }
    );
}

#[wasm_bindgen_test]
fn native_tree_family_rectangles_preserve_outside_and_negative_anchors() {
    for (role, id, width, height, anchor) in [
        (AssetRole::TreePalm, 4653, 88, 168, [20, 175]),
        (AssetRole::TreeConifer, 4654, 90, 180, [-17, -23]),
    ] {
        let mut manifest = fixture();
        let frame = &mut manifest.frames[0];
        frame.source = format!("graphics.drs:[32, 112, 108, 115]:{id}");
        frame.width = width;
        frame.height = height;
        frame.anchor_x = anchor[0];
        frame.anchor_y = anchor[1];
        let place = plan(&manifest, &[0], &[AtlasDomain::Objects]).unwrap()[0];
        let actual = record(&manifest.frames[0], place);
        assert_eq!(actual.anchor, anchor.map(|value| value as f32));
        assert_eq!(actual.size, [f32::from(width), f32::from(height)]);
        assert_eq!(actual.atlas.page, 2);
        assert!(
            OPTIONAL_RESOURCE_SOURCES
                .iter()
                .any(|source| source.role == role)
        );
        manifest.frames[0].x = 2047;
        assert!(plan(&manifest, &[0], &[AtlasDomain::Objects]).is_err());
        manifest.frames[0].x = 0;
        manifest.frames[0].width = 0;
        assert!(plan(&manifest, &[0], &[AtlasDomain::Objects]).is_err());
    }
}

#[wasm_bindgen_test]
fn equal_uv_on_five_pages_copy_masks_alpha_and_keep_source_ids_separate() {
    let mut manifest = fixture();
    let frame = &mut manifest.frames[0];
    // This ID is deliberately unrelated to runtime page 0/1/2: copying only
    // reads the already decoded source slices, not a runtime-indexed source.
    frame.page = 42;
    frame.width = 3;
    frame.height = 1;
    let mut color = vec![0; GAME_ATLAS_PAGE_BYTES];
    let mut player = vec![0; GAME_ATLAS_PAGE_BYTES];
    let mut shadow = vec![0; GAME_ATLAS_PAGE_BYTES];
    let mut pixels = vec![0; GAME_ATLAS_BYTES];
    player[4..8].copy_from_slice(&[7, 0, 0, 1]);
    color[4..8].copy_from_slice(&[99, 99, 99, 99]);
    shadow[8..12].copy_from_slice(&[1, 2, 3, 80]);
    let mut addresses = Vec::new();
    for page in 0..5 {
        color[..4].copy_from_slice(&[page as u8 + 10, 20, 30, 120]);
        let placed = Placement {
            page,
            x: 2,
            y: 2,
            width: 3,
            height: 1,
        };
        copy(frame, placed, &color, &player, &shadow, &mut pixels).unwrap();
        addresses.push(record(frame, placed).atlas);
        let offset =
            usize::from(page) * GAME_ATLAS_PAGE_BYTES + (2 * GAME_ATLAS_SIDE as usize + 2) * 4;
        assert_eq!(
            &pixels[offset..offset + 12],
            &[page as u8 + 10, 20, 30, 120, 99, 99, 100, 100, 1, 2, 3, 80]
        );
        assert_eq!(
            &pixels[usize::from(page) * GAME_ATLAS_PAGE_BYTES..][..4],
            &[0; 4]
        );
    }
    assert!(
        addresses
            .windows(2)
            .all(|pair| pair[0].uv == pair[1].uv && pair[0].page != pair[1].page)
    );
    let bad = Placement {
        page: 5,
        x: 2,
        y: 2,
        width: 3,
        height: 1,
    };
    assert!(copy(frame, bad, &color, &player, &shadow, &mut pixels).is_err());
    assert!(
        copy(
            frame,
            Placement { page: 0, ..bad },
            &color[..4],
            &player,
            &shadow,
            &mut pixels
        )
        .is_err()
    );
}

#[wasm_bindgen_test]
fn source_groups_are_bounded_by_selection_not_sparse_page_id_and_do_not_reorder_records() {
    let mut value = manifest::fixture();
    let template = value["frames"][0].clone();
    value["frames"] = serde_json::json!([
        template.clone(),
        template.clone(),
        template.clone(),
        template
    ]);
    for (index, page) in [65535, 4, 65535, 0].into_iter().enumerate() {
        value["frames"][index]["page"] = page.into();
    }
    let manifest: Manifest = serde_json::from_value(value).unwrap();
    let selected = [2, 1, 0, 3];
    assert_eq!(source_order(&manifest, &selected), [3, 1, 0, 2]);
    assert_eq!(selected, [2, 1, 0, 3]);
}

#[wasm_bindgen_test]
fn reviewed_catalog_is_complete_in_original_source_and_frame_order() {
    let mut value = manifest::fixture();
    let template = value["frames"][0].clone();
    let mut frames = Vec::new();
    let mut expected = Vec::new();
    for selection in REQUIRED_RENDER_SOURCES
        .iter()
        .chain(&OPTIONAL_RESOURCE_SOURCES)
        .chain(&OPTIONAL_TERRAIN_SOURCES)
    {
        let start = frames.len();
        for frame in (0..selection.frames).rev() {
            let mut entry = template.clone();
            entry["source"] = selection.manifest_source().into();
            entry["frame"] = frame.into();
            frames.push(entry);
        }
        expected.extend((start..frames.len()).rev());
    }
    value["frames"] = frames.into();
    let manifest: Manifest = serde_json::from_value(value).unwrap();
    let actual = REQUIRED_RENDER_SOURCES
        .iter()
        .chain(&OPTIONAL_RESOURCE_SOURCES)
        .chain(&OPTIONAL_TERRAIN_SOURCES)
        .flat_map(|source| {
            source_frames(&manifest, &source.manifest_source(), source.frames).unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual.len(),
        aoe_assets::catalog::MAX_REVIEWED_FRAMES as usize
    );
    assert_eq!(actual, expected);
    assert_eq!(TERRAIN_ROLES[4], AssetRole::Rock);
    assert_eq!(
        WATER_DEPTH_ROLES,
        [
            AssetRole::WaterMedium,
            AssetRole::WaterDeep,
            AssetRole::Ice,
            AssetRole::Shallows,
        ]
    );
}

#[wasm_bindgen_test]
fn sequential_plane_passes_match_single_pass_precedence_for_every_alpha_case() {
    let mut manifest = fixture();
    let frame = &mut manifest.frames[0];
    (frame.x, frame.y, frame.width, frame.height) = (0, 0, 8, 1);
    let mut color = vec![0; GAME_ATLAS_PAGE_BYTES];
    let mut player = vec![0; GAME_ATLAS_PAGE_BYTES];
    let mut shadow = vec![0; GAME_ATLAS_PAGE_BYTES];
    // Bits select visible player, colour and shadow alpha; hidden texels keep
    // nonzero RGB so a partial-texel merge would be observable.
    for case in 0..8_u8 {
        let at = usize::from(case) * 4;
        player[at..at + 4].copy_from_slice(&[case, 9, 9, case & 1]);
        color[at..at + 4].copy_from_slice(&[40 + case, 41, 42, (case >> 1) & 1]);
        shadow[at..at + 4].copy_from_slice(&[80 + case, 81, 82, ((case >> 2) & 1) * 90]);
    }
    let placed = Placement {
        page: 1,
        x: 5,
        y: 7,
        width: 8,
        height: 1,
    };
    let mut pixels = vec![77; GAME_ATLAS_BYTES];
    copy(frame, placed, &color, &player, &shadow, &mut pixels).unwrap();
    let origin = GAME_ATLAS_PAGE_BYTES + (7 * GAME_ATLAS_SIDE as usize + 5) * 4;
    // Original alpha-1 cases now blend, preserving partial coverage and ignoring
    // hidden RGB. Opaque trial masks are covered separately below.
    for (case, expected) in [
        [80, 81, 82, 0],
        [45, 101, 171, 1],
        [42, 41, 42, 1],
        [47, 78, 119, 2],
        [84, 81, 82, 90],
        [58, 130, 220, 1],
        [46, 41, 42, 1],
        [56, 93, 144, 2],
    ]
    .into_iter()
    .enumerate()
    {
        let at = case * 4;
        assert_eq!(
            pixels[origin + at..origin + at + 4],
            expected,
            "case {case}"
        );
    }
    assert_eq!(pixels[origin + 32..origin + 36], [77; 4]);
}

#[wasm_bindgen_test]
fn de_player_mask_weights_blend_over_colour_while_trial_ramps_stay_opaque() {
    let mut manifest = fixture();
    let frame = &mut manifest.frames[0];
    (frame.x, frame.y, frame.width, frame.height) = (0, 0, 6, 1);
    let placed = Placement {
        page: 2,
        x: 0,
        y: 0,
        width: 6,
        height: 1,
    };
    let mut color = vec![0; GAME_ATLAS_PAGE_BYTES];
    let mut player = vec![0; GAME_ATLAS_PAGE_BYTES];
    let shadow = vec![0; GAME_ATLAS_PAGE_BYTES];
    for (i, weight) in [0, 1, 64, 119, 127, 255].into_iter().enumerate() {
        color[i * 4..i * 4 + 4].copy_from_slice(&[117, 120, 115, 255]);
        player[i * 4..i * 4 + 4].copy_from_slice(&[7, 0, 0, weight]);
    }
    let mut pixels = vec![0; GAME_ATLAS_BYTES];
    copy(frame, placed, &color, &player, &shadow, &mut pixels).unwrap();
    // Ramp 7 is [65,145,245]; mask 119 blends it over the neutral roof.
    assert_eq!(
        &pixels[2 * GAME_ATLAS_PAGE_BYTES..][..24],
        &[
            117, 120, 115, 255, 117, 120, 116, 255, 104, 126, 148, 255, 93, 132, 176, 255, 91, 132,
            180, 255, 65, 145, 245, 255,
        ]
    );
}

#[wasm_bindgen_test]
fn cliff_palette_keeps_relief_and_alpha_with_a_neutral_warm_stone_colour() {
    let mut manifest = fixture();
    let frame = &mut manifest.frames[0];
    frame.source = "graphics.drs:[32, 112, 108, 115]:226".to_owned();
    frame.width = 1;
    frame.height = 1;
    frame.x = 0;
    frame.y = 0;
    let place = plan(&manifest, &[0], &[AtlasDomain::Objects]).unwrap()[0];
    let mut source = vec![0; GAME_ATLAS_PAGE_BYTES];
    let mut pixels = vec![0; GAME_ATLAS_BYTES];
    let to = usize::from(place.page) * GAME_ATLAS_PAGE_BYTES
        + (usize::from(place.y) * GAME_ATLAS_SIDE as usize + usize::from(place.x)) * 4;
    for texel in [[190, 115, 35, 255], [45, 30, 12, 128], [0, 0, 0, 0]] {
        source[..4].copy_from_slice(&texel);
        copy_plane(
            &manifest.frames[0],
            place,
            Plane::Color,
            &source,
            &mut pixels,
            [65, 145, 245],
        )
        .unwrap();
        let actual = &pixels[to..to + 4];
        assert_eq!(actual[3], texel[3]);
        assert!(
            i16::from(actual[0]) - i16::from(actual[2]) <= 32,
            "stone, not orange: {actual:?}"
        );
        if texel[3] != 0 {
            assert!(actual[0] >= actual[1] && actual[1] >= actual[2]);
        }
    }
    manifest.frames[0].source = "graphics.drs:[32, 112, 108, 115]:1034".to_owned();
    source[..4].copy_from_slice(&[190, 115, 35, 255]);
    copy_plane(
        &manifest.frames[0],
        place,
        Plane::Color,
        &source,
        &mut pixels,
        [65, 145, 245],
    )
    .unwrap();
    assert_eq!(&pixels[to..to + 4], &[190, 115, 35, 255]);
}

#[wasm_bindgen_test]
fn dressing_catalog_prefixes_preserve_native_signed_hotspots_and_object_page() {
    for (id, count, anchor) in [
        (4474, 6, [28, 6]),
        (1035, 3, [-3, 13]),
        (1252, 2, [63, 18]),
        (1262, 1, [94, 172]),
    ] {
        let source = OPTIONAL_RESOURCE_SOURCES
            .iter()
            .find(|source| source.id == id)
            .unwrap();
        assert_eq!(source.frames, count);
        let mut value = manifest::fixture();
        value["frames"][0]["anchor_x"] = anchor[0].into();
        value["frames"][0]["anchor_y"] = anchor[1].into();
        let manifest: Manifest = serde_json::from_value(value).unwrap();
        let places = plan(&manifest, &[0], &[AtlasDomain::Objects]).unwrap();
        let frame = record(&manifest.frames[0], places[0]);
        assert_eq!(frame.atlas.page, 2);
        assert_eq!(frame.anchor, anchor.map(|v| v as f32));
    }
}

/// The former half-resolution sources (Feudal trial SLPs, shared construction
/// and ambient layers) must keep one atlas texel per source pixel.
#[wasm_bindgen_test]
fn formerly_halved_sources_plan_and_copy_at_full_native_resolution() {
    for id in [
        24, 1009, 2278, 2655, 2235, 133, 741, 737, 733, 895, 903, 3600, 4616, 380, 388, 93, 4518,
        4520, 236, 239, 4397,
    ] {
        let mut manifest = fixture();
        let frame = &mut manifest.frames[0];
        frame.source = format!("graphics.drs:[32, 112, 108, 115]:{id}");
        (frame.x, frame.y, frame.width, frame.height) = (4, 3, 5, 3);
        let place = plan(&manifest, &[0], &[AtlasDomain::Objects]).unwrap()[0];
        assert_eq!((place.width, place.height), (5, 3), "SLP {id}");
        let mut source = vec![0; GAME_ATLAS_PAGE_BYTES];
        for row in 0..3_usize {
            for col in 0..5_usize {
                let at = ((3 + row) * GAME_ATLAS_SIDE as usize + 4 + col) * 4;
                source[at..at + 4].copy_from_slice(&[1 + col as u8, 1 + row as u8, id as u8, 255]);
            }
        }
        let mut pixels = vec![0; GAME_ATLAS_BYTES];
        copy_plane(
            &manifest.frames[0],
            place,
            Plane::Color,
            &source,
            &mut pixels,
            [65, 145, 245],
        )
        .unwrap();
        for row in 0..3_usize {
            for col in 0..5_usize {
                let at = usize::from(place.page) * GAME_ATLAS_PAGE_BYTES
                    + ((usize::from(place.y) + row) * GAME_ATLAS_SIDE as usize
                        + usize::from(place.x)
                        + col)
                        * 4;
                assert_eq!(
                    &pixels[at..at + 4],
                    &[1 + col as u8, 1 + row as u8, id as u8, 255],
                    "SLP {id} texel {col},{row}"
                );
            }
        }
        // A copy rectangle that still assumed two source pixels per texel fails.
        let mut halved = place;
        halved.width = 3;
        assert!(
            copy_plane(
                &manifest.frames[0],
                halved,
                Plane::Color,
                &source,
                &mut pixels,
                [65, 145, 245]
            )
            .is_err()
        );
    }
}

#[wasm_bindgen_test]
fn repeated_masked_frames_share_storage_and_upload_once_within_the_fixed_budget() {
    let mut manifest = fixture();
    manifest.frames[0].width = 1800;
    manifest.frames[0].height = 1800;
    let selected = [0; 6];
    let domains = [
        AtlasDomain::Objects,
        AtlasDomain::Objects,
        AtlasDomain::Objects,
        AtlasDomain::Terrain,
        AtlasDomain::Shared,
        AtlasDomain::Shared,
    ];
    let places = plan(&manifest, &selected, &domains).unwrap();
    assert_eq!(places.len(), 6, "all semantic entries survive");
    assert_eq!(copy_order(&manifest, &selected, &places), [0, 3, 4, 5]);
    assert_eq!(places[0], places[1]);
    assert_eq!(places[1], places[2]);
    assert_eq!(places[0].page, 2);
    assert_eq!(places[3].page, 0);
    assert_ne!(
        places[4], places[5],
        "unmasked entries can bake different colours"
    );
}
