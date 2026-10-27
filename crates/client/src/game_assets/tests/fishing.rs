use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn fixture(id: u32, count: u32) -> Manifest {
    let mut json = manifest::fixture();
    let base = json["frames"][0].clone();
    json["frames"] = serde_json::Value::Array(
        (0..count)
            .map(|frame| {
                let mut value = base.clone();
                value["source"] = format!("graphics.drs:[32, 112, 108, 115]:{id}").into();
                value["frame"] = frame.into();
                value
            })
            .collect(),
    );
    serde_json::from_value(json).unwrap()
}

#[wasm_bindgen_test]
fn fishing_sources_preserve_native_rows_and_reject_present_truncation() {
    assert!(
        fishing::select(&fixture(0, 1), &mut Vec::new(), &mut Vec::new())
            .unwrap()
            .iter()
            .all(Option::is_none),
        "unused slot is not sprite source zero"
    );
    let manifest = fixture(3575, 140);
    let mut selected = Vec::new();
    let ranges = actors::select(&manifest, &mut selected, &mut Vec::new()).unwrap();
    let range = ranges[aoe_assets::catalog::units::Pose::FisherWork as usize]
        .clone()
        .unwrap();
    assert_eq!(range, 0..70);
    assert_eq!(&selected[..4], &[0, 2, 4, 6]);
    assert_eq!(selected[14], 28, "next native facing row");
    assert!(actors::select(&fixture(444, 4), &mut Vec::new(), &mut Vec::new()).is_err());
    let mut selected = Vec::new();
    let ranges = fishing::select(&fixture(1911, 49), &mut selected, &mut Vec::new()).unwrap();
    assert_eq!(ranges[2], Some(0..7));
    assert_eq!(selected, [0, 7, 14, 21, 28, 35, 42]);
}

#[wasm_bindgen_test]
fn complete_native_and_de_selection_fits_the_unchanged_atlas_budget() {
    use aoe_assets::catalog::units::SPRITES;
    let catalog = REQUIRED_RENDER_SOURCES
        .iter()
        .chain(&OPTIONAL_RESOURCE_SOURCES)
        .chain(&OPTIONAL_TERRAIN_SOURCES);
    let mut sources = std::collections::BTreeMap::new();
    for source in catalog.clone() {
        sources.insert(source.manifest_source(), source.frames);
    }
    for source in SPRITES {
        sources.insert(source.manifest_source(), source.frames());
    }
    for (id, count) in [
        (1496, 75),
        (3483, 1),
        (3482, 43),
        (3549, 34),
        (1910, 49),
        (1911, 49),
        (1912, 49),
        (1913, 49),
        (420, 30),
        (446, 5),
    ] {
        sources.insert(format!("graphics.drs:[32, 112, 108, 115]:{id}"), count);
    }
    for id in 424..=432 {
        sources.insert(format!("graphics.drs:[32, 112, 108, 115]:{id}"), 20);
    }
    sources.insert("de/b_misc_waypoint_flag_x1.sld".into(), 90);
    for stem in [
        "b_dark_house_age1",
        "b_west_lumber_camp_age2",
        "b_west_mining_camp_age2",
        "b_dark_barracks_age1",
        "b_dark_wall_palisade",
        "b_dark_outpost_age1",
        "b_dark_dock_age1",
        "b_dark_town_center_age1",
    ] {
        sources.insert(
            format!("de/{stem}_x1.sld"),
            if stem.contains("house") {
                3
            } else if stem.contains("palisade") {
                5
            } else {
                1
            },
        );
        sources.insert(format!("de/{stem}_rubble_x1.sld"), 1);
    }
    sources.insert("de/b_dark_mill_age1_rubble_x1.sld".into(), 1);
    for side in 1..=4 {
        sources.insert(format!("de/b_misc_foundation_{side}x{side}_x1.sld"), 3);
    }
    sources.insert("de/b_misc_foundation_dock_x1.sld".into(), 3);
    sources.insert("de/b_misc_rubble_x1.sld".into(), 1);
    sources.insert("de/g_fm1.dds".into(), 1);
    let mut value = manifest::fixture();
    let base = value["frames"][0].clone();
    value["frames"] = sources
        .into_iter()
        .flat_map(|(source, count)| {
            let base = base.clone();
            (0..count).map(move |index| {
                let mut frame = base.clone();
                frame["source"] = source.clone().into();
                frame["frame"] = index.into();
                frame["width"] = 2.into();
                frame["height"] = 2.into();
                frame
            })
        })
        .collect::<Vec<_>>()
        .into();
    let mut manifest: Manifest = serde_json::from_value(value).unwrap();
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    for source in catalog {
        let frames = compact_standing(
            source_frames(&manifest, &source.manifest_source(), source.frames).unwrap(),
            source.role,
        );
        domains.extend(std::iter::repeat_n(
            if source.terrain_topology.is_some() {
                AtlasDomain::Terrain
            } else {
                AtlasDomain::Objects
            },
            frames.len(),
        ));
        selected.extend(frames);
    }
    actors::select(&manifest, &mut selected, &mut domains).unwrap();
    buildings::select(&manifest, &mut selected, &mut domains).unwrap();
    building_sprites::select(&manifest, &mut selected, &mut domains).unwrap();
    fishing::select(&manifest, &mut selected, &mut domains).unwrap();
    // Combined Castle poses add 365 cells to the naval/DAT/rally selection;
    // the Castle camp fallback slots reuse two available DE camp cells.
    // King poses add 35 selected cells to the current catalog. Regenerate this
    // content-count pin; the 5120-frame and five-page atlas gates stay fixed.
    // The stationary and working fishing ship share one identical five-frame group.
    assert_eq!(selected.len(), 5092);
    assert!(selected.len() <= MAX_SELECTED_FRAMES);
    assert_eq!(
        placement::plan(&manifest, &selected, &domains)
            .unwrap()
            .len(),
        selected.len()
    );
    building_sprites::rally(&manifest, &mut selected, &mut domains).unwrap();
    let parts = (424..=432)
        .map(|id| {
            let source = format!("graphics.drs:[32, 112, 108, 115]:{id}");
            manifest::BuildingPart {
                record: source_frames(&manifest, &source, 20).map(|frames| frames[0]),
                frames: 20,
                angles: 1,
                primary: false,
                frame_rate: 0.05,
                offset: [0; 2],
                misplacement: [0.0; 2],
            }
        })
        .collect();
    manifest.buildings.push(manifest::BuildingComposition {
        unit_id: 70,
        footprint: [2.0; 2],
        foundation_slp: None,
        foundation_size: [0; 2],
        parts: Vec::new(),
        construction: Vec::new(),
        damage: vec![manifest::BuildingDamage {
            damage_percent: 25,
            apply_mode: 0,
            parts,
        }],
    });
    buildings::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(selected.len(), MAX_SELECTED_FRAMES);
    assert_eq!(
        placement::plan(&manifest, &selected, &domains)
            .unwrap()
            .len(),
        selected.len()
    );
}
