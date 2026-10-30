use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn fixture(id: u32, count: u32) -> Manifest {
    let mut json = manifest::fixture();
    let base = json["frames"][0].clone();
    json["frames"] = serde_json::Value::Array(
        (0..count)
            .map(|frame| {
                let mut value = base.clone();
                value["source"] = aoe_assets::catalog::units::SPRITES
                    .iter()
                    .find(|s| s.id == id)
                    .map_or_else(
                        || format!("graphics.drs:[32, 112, 108, 115]:{id}"),
                        |s| s.manifest_source(),
                    )
                    .into();
                value["frame"] = frame.into();
                value
            })
            .collect(),
    );
    serde_json::from_value(json).unwrap()
}

#[wasm_bindgen_test]
fn fishing_worker_preserves_native_rows_and_rejects_present_truncation() {
    let source =
        aoe_assets::catalog::units::SPRITES[aoe_assets::catalog::units::Pose::FisherWork as usize];
    let manifest = fixture(source.id, source.frames());
    let mut selected = Vec::new();
    let ranges = actors::select(&manifest, &mut selected, &mut Vec::new()).unwrap();
    let range = ranges[aoe_assets::catalog::units::Pose::FisherWork as usize]
        .clone()
        .unwrap();
    assert_eq!(range, 0..960);
    assert_eq!(&selected[..4], &[0, 1, 2, 3]);
    assert_eq!(selected[60], 60, "next native facing row");
    assert!(
        actors::select(
            &fixture(
                aoe_assets::catalog::units::SPRITES
                    [aoe_assets::catalog::units::Pose::FishingShipStand as usize]
                    .id,
                15
            ),
            &mut Vec::new(),
            &mut Vec::new()
        )
        .is_err()
    );
}

#[wasm_bindgen_test]
fn complete_actor_inventory_and_dat_damage_cells_fit_without_startup_animation_pages() {
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
    let damage_sources: Vec<_> = (0..9)
        .map(|i| format!("de/synthetic_damage_{i}.sld"))
        .collect();
    for source in &damage_sources {
        sources.insert(source.clone(), 20);
    }
    let native: std::collections::BTreeSet<_> = SPRITES
        .iter()
        .filter(|s| s.is_native())
        .map(|s| s.manifest_source())
        .chain(damage_sources.iter().cloned())
        .collect();
    let mut value = manifest::fixture();
    let base = value["frames"][0].clone();
    let frames: Vec<_> = sources
        .into_iter()
        .flat_map(|(source, count)| {
            let base = base.clone();
            (0..count).map(move |index| {
                let mut f = base.clone();
                f["source"] = source.clone().into();
                f["frame"] = index.into();
                f["width"] = 2.into();
                f["height"] = 2.into();
                f["page"] = 0.into();
                f["x"] = 0.into();
                f["y"] = 0.into();
                f
            })
        })
        .enumerate()
        .map(|(index, mut f)| {
            if native.contains(f["source"].as_str().unwrap()) {
                f["native"] = serde_json::json!({"chunk":0,"offset":index*64,"length":64});
            }
            f
        })
        .collect();
    value["chunks"] = serde_json::json!([{"file":"frames-0000.bin","hash":"0".repeat(64),"bytes":frames.len()*64}]);
    value["frames"] = frames.into();
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
    let ranges = actors::select(&manifest, &mut selected, &mut domains).unwrap();
    assert!(ranges.iter().all(Option::is_some));
    for (range, source) in ranges.iter().zip(SPRITES) {
        assert_eq!(range.as_ref().unwrap().len(), source.selected_frames());
    }
    assert_eq!(
        selected.len(),
        94_729,
        "current complete catalog, sharing identical pose ranges"
    );
    let before = selected.len();
    let parts = (0..27)
        .map(|i| manifest::BuildingPart {
            record: Some(source_frames(&manifest, &damage_sources[i % 9], 20).unwrap()[0]),
            frames: 2,
            angles: 1,
            primary: false,
            replay_delay: 0.0,
            frame_rate: 0.1,
            offset: [i as i32, 0],
            misplacement: [0.0; 2],
        })
        .collect();
    manifest.buildings.push(manifest::BuildingComposition {
        unit_id: 72,
        footprint: [1.0; 2],
        foundation_slp: None,
        foundation_size: [0; 2],
        parts: Vec::new(),
        states: Vec::new(),
        construction: Vec::new(),
        damage: vec![manifest::BuildingDamage {
            damage_percent: 25,
            apply_mode: 0,
            parts,
        }],
    });
    buildings::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(
        selected.len(),
        before + 180,
        "every native source cell, including cells outside the declared loop"
    );
    let placements = placement::plan(&manifest, &selected, &domains).unwrap();
    assert_eq!(placements.len(), selected.len());
    assert_eq!(
        placement::pages(&placements),
        GAME_ATLAS_PAGES,
        "native animations allocate through residency only"
    );
}
