use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn fixture() -> Manifest {
    serde_json::from_value(manifest::fixture()).unwrap()
}

#[wasm_bindgen_test]
fn count_and_source_bounds_are_checked_before_destination_allocation() {
    let manifest = fixture();
    let selected = vec![0; MAX_SELECTED_FRAMES];
    let domains = vec![AtlasDomain::Terrain; MAX_SELECTED_FRAMES];
    assert_eq!(plan(&manifest, &selected, &domains).unwrap().len(), 2048);
    assert!(plan(&manifest, &vec![0; 2049], &vec![AtlasDomain::Terrain; 2049]).is_err());
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
fn equal_uv_on_three_pages_copy_masks_alpha_and_keep_source_ids_separate() {
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
    for page in 0..3 {
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
            &[page as u8 + 10, 20, 30, 120, 65, 145, 245, 255, 1, 2, 3, 80]
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
        page: 3,
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
fn reviewed_catalog_is_exactly_656_in_original_source_and_frame_order() {
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
    assert_eq!(actual.len(), 656);
    assert_eq!(actual, expected);
    assert_eq!(TERRAIN_ROLES[4], AssetRole::Rock);
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
    for case in 0..8_usize {
        let at = case * 4;
        let expected = if player[at + 3] > 0 {
            let shade = 0.65 + f32::from(player[at].min(7)) / 7.0 * 0.35;
            [
                (65.0 * shade) as u8,
                (145.0 * shade) as u8,
                (245.0 * shade) as u8,
                255,
            ]
        } else if color[at + 3] > 0 {
            color[at..at + 4].try_into().unwrap()
        } else {
            shadow[at..at + 4].try_into().unwrap()
        };
        assert_eq!(
            pixels[origin + at..origin + at + 4],
            expected,
            "case {case}"
        );
    }
    assert_eq!(pixels[origin + 32..origin + 36], [77; 4]);
}
