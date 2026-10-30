use super::*;
use aoe_assets::catalog::units::{Pose, SPRITES};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn siege_stone_atlas_keeps_every_native_cell_and_rejects_changed_topology() {
    let mut value = manifest::fixture();
    let template = value["frames"][0].clone();
    let cells = |count: u32| {
        (0..count)
            .map(|frame| {
                let mut cell = template.clone();
                cell["source"] = "de/p_mangonel_x1.sld".into();
                cell["frame"] = frame.into();
                cell["x"] = (frame * 3).into();
                cell
            })
            .collect::<Vec<_>>()
    };
    value["frames"] = cells(30).into();
    let manifest: Manifest = serde_json::from_value(value.clone()).unwrap();
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    let ranges = actors::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(selected, (0..30).collect::<Vec<_>>());
    let places = placement::plan(&manifest, &selected, &domains).unwrap();
    let records = selected
        .iter()
        .zip(places)
        .map(|(&index, place)| placement::record(&manifest.frames[index], place))
        .collect::<Vec<_>>();
    assert_eq!(
        actors::resolve(&manifest, &selected, &records, &ranges)[Pose::Stone as usize].len(),
        30
    );
    for count in [29, 31] {
        value["frames"] = cells(count).into();
        let unsupported: Manifest = serde_json::from_value(value.clone()).unwrap();
        assert!(actors::select(&unsupported, &mut Vec::new(), &mut Vec::new()).is_err());
    }
}

#[wasm_bindgen_test]
fn base_cavalry_cells_are_reused_without_changing_light_cavalry_sampling() {
    let mut value = manifest::fixture();
    let template = value["frames"][0].clone();
    value["frames"] = [3004, 3008]
        .into_iter()
        .flat_map(|id| {
            let template = template.clone();
            (0..50).map(move |frame| {
                let mut f = template.clone();
                f["source"] = format!("graphics.drs:[32, 112, 108, 115]:{id}").into();
                f["frame"] = frame.into();
                f["x"] = (frame * 3).into();
                f
            })
        })
        .collect::<Vec<_>>()
        .into();
    let manifest: Manifest = serde_json::from_value(value).unwrap();
    let mut selected = vec![0, 10, 20, 30, 40];
    selected.extend(50..100);
    let mut domains = vec![AtlasDomain::Objects; selected.len()];
    let ranges = actors::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(
        selected.len(),
        55,
        "both base groups already reside in the atlas"
    );
    let places = placement::plan(&manifest, &selected, &domains).unwrap();
    let records: Vec<_> = selected
        .iter()
        .zip(places)
        .map(|(&i, p)| placement::record(&manifest.frames[i], p))
        .collect();
    let groups = actors::resolve(&manifest, &selected, &records, &ranges);
    for pose in [Pose::LightCavalryStand, Pose::LightCavalryWalk] {
        let source = SPRITES[pose as usize];
        let expected: Vec<_> = (0..5)
            .flat_map(|row| {
                (0..usize::from(source.stride))
                    .step_by(usize::from(source.sample_step()))
                    .map(move |phase| source.source_frame(row, phase))
            })
            .collect();
        assert_eq!(groups[pose as usize].len(), expected.len());
        for (actual, frame) in groups[pose as usize].iter().zip(expected) {
            let index = manifest
                .frames
                .iter()
                .position(|f| f.source == source.manifest_source() && f.frame == frame as u32)
                .unwrap();
            let slot = selected.iter().position(|&i| i == index).unwrap();
            assert_eq!(actual.atlas.uv, records[slot].atlas.uv);
            assert_eq!(actual.anchor, records[slot].anchor);
        }
    }
}

#[wasm_bindgen_test]
fn sheep_neutral_and_owned_frames_share_sources_but_have_distinct_player_colours() {
    let source = SPRITES[Pose::SheepStand as usize];
    let mut value = manifest::fixture();
    let template = value["frames"][0].clone();
    let frames = (0..source.frames())
        .map(|frame| {
            let mut f = template.clone();
            f["source"] = source.manifest_source().into();
            f["frame"] = frame.into();
            f
        })
        .collect::<Vec<_>>();
    value["frames"] = frames.into();
    let manifest: Manifest = serde_json::from_value(value.clone()).unwrap();
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    let ranges = actors::select(&manifest, &mut selected, &mut domains).unwrap();
    let owned = ranges[Pose::SheepStand as usize].as_ref().unwrap().start;
    let neutral = ranges[Pose::NeutralSheepStand as usize]
        .as_ref()
        .unwrap()
        .start;
    assert_ne!(owned, neutral);
    assert_eq!(selected[owned], selected[neutral]);
    assert_eq!(
        actors::colour(&ranges, owned, [65, 145, 245]),
        [65, 145, 245]
    );
    assert_eq!(
        actors::colour(&ranges, neutral, [65, 145, 245]),
        [190, 190, 190]
    );
    value["frames"].as_array_mut().unwrap().pop();
    let bad: Manifest = serde_json::from_value(value).unwrap();
    assert!(actors::select(&bad, &mut Vec::new(), &mut Vec::new()).is_err());
}

#[wasm_bindgen_test]
fn repeated_scout_cells_are_static_but_changed_pixels_and_hotspots_keep_the_loop() {
    let source = SPRITES[Pose::ScoutStand as usize];
    let mut value = manifest::fixture();
    let template = value["frames"][0].clone();
    value["frames"] = (0..source.frames())
        .map(|frame| {
            let mut cell = template.clone();
            cell["source"] = source.manifest_source().into();
            cell["frame"] = frame.into();
            // Each facing has its own image; only phases within a facing repeat.
            cell["x"] = (frame / u32::from(source.stride) * 10).into();
            cell
        })
        .collect::<Vec<_>>()
        .into();
    let mut manifest: Manifest = serde_json::from_value(value).unwrap();
    let resolve = |manifest: &Manifest| {
        let (mut selected, mut domains) = (Vec::new(), Vec::new());
        let ranges = actors::select(manifest, &mut selected, &mut domains).unwrap();
        let places = placement::plan(manifest, &selected, &domains).unwrap();
        let records = selected
            .iter()
            .zip(places)
            .map(|(&index, placed)| placement::record(&manifest.frames[index], placed))
            .collect::<Vec<_>>();
        actors::resolve(manifest, &selected, &records, &ranges)
    };
    let frames = resolve(&manifest);
    let scout = &frames[Pose::ScoutStand as usize];
    assert_eq!(
        scout.len(),
        5,
        "identical phases must not animate the world"
    );
    for pair in scout.windows(2) {
        assert_ne!(pair[0].atlas.uv, pair[1].atlas.uv, "retain every facing");
    }
    let phase = source.source_frame(2, usize::from(source.sample_step()));
    manifest.frames[phase].x += 1;
    assert_eq!(
        resolve(&manifest)[Pose::ScoutStand as usize].len(),
        source.selected_frames(),
        "one changed source cell preserves the native loop"
    );
    manifest.frames[phase].x -= 1;
    manifest.frames[phase].anchor_x += 1;
    assert_eq!(
        resolve(&manifest)[Pose::ScoutStand as usize].len(),
        source.selected_frames(),
        "hotspot motion is visible animation even with identical pixels"
    );
}

#[wasm_bindgen_test]
fn vassal_king_groups_select_bounded_native_facings_and_refuse_partial_art() {
    let poses = [
        Pose::KingStand,
        Pose::KingWalk,
        Pose::KingDie,
        Pose::KingDecay,
    ];
    let mut value = manifest::fixture();
    let template = value["frames"][0].clone();
    value["frames"] = poses
        .into_iter()
        .flat_map(|pose| {
            let source = SPRITES[pose as usize];
            let template = template.clone();
            (0..source.frames()).map(move |frame| {
                let mut cell = template.clone();
                cell["source"] = source.manifest_source().into();
                cell["frame"] = frame.into();
                cell
            })
        })
        .collect::<Vec<_>>()
        .into();
    let manifest: Manifest = serde_json::from_value(value.clone()).unwrap();
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    let ranges = actors::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(selected.len(), 35);
    for pose in poses {
        let source = SPRITES[pose as usize];
        let range = ranges[pose as usize].clone().unwrap();
        let actual: Vec<_> = selected[range]
            .iter()
            .map(|&i| manifest.frames[i].frame)
            .collect();
        let expected: Vec<_> = (0..5)
            .flat_map(|row| {
                (0..usize::from(source.stride))
                    .step_by(usize::from(source.sample_step()))
                    .map(move |phase| source.source_frame(row, phase) as u32)
            })
            .collect();
        assert_eq!(actual, expected);
    }
    value["frames"].as_array_mut().unwrap().pop();
    let partial: Manifest = serde_json::from_value(value).unwrap();
    assert!(actors::select(&partial, &mut Vec::new(), &mut Vec::new()).is_err());
}

#[wasm_bindgen_test]
fn identical_owned_actor_groups_share_their_selected_frames() {
    let source = SPRITES[Pose::FishingShipStand as usize];
    let mut value = manifest::fixture();
    let template = value["frames"][0].clone();
    value["frames"] = (0..source.frames())
        .map(|frame| {
            let mut cell = template.clone();
            cell["source"] = source.manifest_source().into();
            cell["frame"] = frame.into();
            cell
        })
        .collect::<Vec<_>>()
        .into();
    let manifest: Manifest = serde_json::from_value(value).unwrap();
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    let ranges = actors::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(
        ranges[Pose::FishingShipStand as usize],
        ranges[Pose::FishingShipWork as usize]
    );
    assert_eq!(selected.len(), 5);
}

#[wasm_bindgen_test]
fn projectile_atlas_uses_level_flight_frames_in_each_native_direction() {
    let mut value = manifest::fixture();
    let template = value["frames"][0].clone();
    value["frames"] = [Pose::Arrow, Pose::Javelin]
        .into_iter()
        .flat_map(|pose| {
            let source = SPRITES[pose as usize];
            let template = template.clone();
            (0..source.frames()).map(move |frame| {
                let mut cell = template.clone();
                cell["source"] = source.manifest_source().into();
                cell["frame"] = frame.into();
                cell
            })
        })
        .collect::<Vec<_>>()
        .into();
    let manifest: Manifest = serde_json::from_value(value).unwrap();
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    let ranges = actors::select(&manifest, &mut selected, &mut domains).unwrap();
    for (pose, expected) in [
        (Pose::Arrow, [6, 49, 93, 137, 175]),
        (Pose::Javelin, [5, 45, 84, 125, 169]),
    ] {
        let range = ranges[pose as usize].clone().unwrap();
        let actual = selected[range]
            .iter()
            .map(|&index| manifest.frames[index].frame)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }
}

#[wasm_bindgen_test]
fn general_hero_groups_select_bounded_native_facings_and_refuse_partial_art() {
    let poses = [
        Pose::GeneralStand,
        Pose::GeneralWalk,
        Pose::GeneralAttack,
        Pose::GeneralDie,
        Pose::GeneralDecay,
    ];
    let mut value = manifest::fixture();
    let template = value["frames"][0].clone();
    value["frames"] = poses
        .into_iter()
        .map(|pose| SPRITES[pose as usize])
        .map(|source| (source.id, source))
        .collect::<std::collections::BTreeMap<_, _>>()
        .into_values()
        .flat_map(|source| {
            let template = template.clone();
            (0..source.frames()).map(move |frame| {
                let mut cell = template.clone();
                cell["source"] = source.manifest_source().into();
                cell["frame"] = frame.into();
                cell
            })
        })
        .collect::<Vec<_>>()
        .into();
    let manifest: Manifest = serde_json::from_value(value.clone()).unwrap();
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    let ranges = actors::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(selected.len(), 3120);
    for pose in poses {
        let source = SPRITES[pose as usize];
        let range = ranges[pose as usize].clone().unwrap();
        let actual: Vec<_> = selected[range]
            .iter()
            .map(|&i| manifest.frames[i].frame)
            .collect();
        let expected: Vec<_> = (0..source.rows())
            .flat_map(|row| {
                (0..usize::from(source.stride))
                    .step_by(usize::from(source.sample_step()))
                    .map(move |phase| source.source_frame(row, phase) as u32)
            })
            .collect();
        assert_eq!(actual, expected);
    }
    let mut extra = value["frames"][0].clone();
    extra["frame"] = SPRITES[Pose::GeneralStand as usize].frames().into();
    value["frames"].as_array_mut().unwrap().push(extra);
    let unsupported: Manifest = serde_json::from_value(value.clone()).unwrap();
    assert!(actors::select(&unsupported, &mut Vec::new(), &mut Vec::new()).is_err());
    value["frames"].as_array_mut().unwrap().pop();
    value["frames"].as_array_mut().unwrap().pop();
    let partial: Manifest = serde_json::from_value(value).unwrap();
    assert!(actors::select(&partial, &mut Vec::new(), &mut Vec::new()).is_err());
}
