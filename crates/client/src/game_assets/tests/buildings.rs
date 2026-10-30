use super::selection_tests::{FetchFixture, pack_fixture};
use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn compile(value: &mut serde_json::Value) {
    let buildings: Vec<aoe_assets::dat::BuildingComposition> =
        serde_json::from_value(value["buildings"].clone()).unwrap();
    let frames: Vec<aoe_assets::pack::FrameRecord> =
        serde_json::from_value(value["frames"].clone()).unwrap();
    value["building_data"] = aoe_assets::pack::recipes::encode(&buildings, &frames)
        .unwrap()
        .into();
}

#[wasm_bindgen_test]
async fn castle_fallbacks_and_rubble_keep_native_geometry_only_bridges_are_halved() {
    let extras = [
        ("de/b_west_castle_age3_x1.sld", 1),
        ("de/b_west_castle_age3_rubble_x1.sld", 1),
        ("graphics.drs:[32, 112, 108, 115]:3620", 1),
        ("de/b_west_lumber_camp_age2_x1.sld", 1),
        ("de/b_west_mining_camp_age2_x1.sld", 1),
        ("de/b_west_dock_age3_x1.sld", 1),
        ("graphics.drs:[32, 112, 108, 115]:281", 1),
        ("graphics.drs:[32, 112, 108, 115]:3688", 1),
        ("graphics.drs:[32, 112, 108, 115]:3690", 1),
        ("graphics.drs:[32, 112, 108, 115]:3692", 1),
        ("graphics.drs:[32, 112, 108, 115]:3694", 1),
        ("graphics.drs:[32, 112, 108, 115]:3696", 1),
        ("graphics.drs:[32, 112, 108, 115]:3698", 1),
    ];
    let mut value = pack_fixture(
        REQUIRED_RENDER_SOURCES
            .iter()
            .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some()))
            .chain(extras.map(|(source, count)| (source.to_owned(), count, false))),
    );
    for record in value["frames"].as_array_mut().unwrap() {
        // A signed, noncentral hotspot detects anchor scaling as well as size.
        record["anchor_x"] = (-3).into();
        record["anchor_y"] = 7.into();
    }
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load_collected().await.unwrap();
    drop(fetch);
    for slot in [45, 46, 49, 53, 55, 56, 59, 64] {
        let frame = art.buildings[slot]
            .first()
            .expect("Castle fallback/rubble is loaded");
        assert_eq!(frame.size, [1.0, 1.0], "slot {slot}");
        assert_eq!(frame.anchor, [-3.0, 7.0], "slot {slot}");
    }
    for slot in 65..=70 {
        let frame = art.buildings[slot].first().expect("bridge is loaded");
        assert_eq!(frame.size, [0.5, 0.5], "slot {slot}");
        assert_eq!(frame.anchor, [-1.5, 3.5], "slot {slot}");
    }
}

#[wasm_bindgen_test]
async fn non_gameplay_buildings_do_not_allocate_the_gameplay_atlas() {
    let mut value = pack_fixture(
        REQUIRED_RENDER_SOURCES
            .iter()
            .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some())),
    );
    // An unused DAT building must not reserve live atlas space.
    value["buildings"] = serde_json::json!([{"unit_id":999,"footprint":[3.0,3.0],"foundation_terrain":-1,"foundation_slp":null,"foundation_size":[0,0],"parts":[{"graphic_id":1,"slp_id":99999,"source":"absent.slp","available":false,"frames":0,"angles":1,"primary":true,"replay_delay":0.0,"frame_rate":0.0,"offset":[0,0],"misplacement":[0.0,0.0],"layer":20}],"damage":[],"states":[],"construction":[]}]);
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load_collected()
        .await
        .expect("only gameplay recipes are resident");
    drop(fetch);
    assert!(art.compositions.is_empty());
}

#[wasm_bindgen_test]
async fn dock_keeps_dat_composition_construction_and_foundation_metadata() {
    let source = "graphics.drs:[32, 112, 108, 115]:90000".to_owned();
    let mut value = pack_fixture(
        REQUIRED_RENDER_SOURCES
            .iter()
            .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some()))
            .chain([(source.clone(), 3, false)]),
    );
    let part = serde_json::json!({"graphic_id":1,"slp_id":90000,"source":source,
        "available":true,"frames":1,"angles":3,"primary":true,"replay_delay":0.0,"frame_rate":0.0,
        "offset":[-9,11],"misplacement":[0.0,0.0],"layer":20});
    value["buildings"] = serde_json::json!([{"unit_id":45,"footprint":[3.0,3.0],
        "foundation_terrain":27,"foundation_slp":15006,"foundation_size":[10,10],
        "parts":[part.clone()],"damage":[],"states":[],"construction":[part]}]);
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load_collected().await.expect("Dock is a live DAT building");
    drop(fetch);
    let dock = art.compositions.iter().find(|b| b.unit_id == 45).unwrap();
    assert_eq!(dock.foundation_material, Some(13));
    assert_eq!(dock.parts[0].offset, [-9.0, 11.0]);
    assert_eq!(dock.construction[0].angles, 3);
    assert_eq!(dock.construction[0].animation.len(), 3);
    assert!(art.buildings[23].is_empty());
    assert!(art.buildings[24].is_empty());
}

#[wasm_bindgen_test]
async fn dock_wake_preserves_every_native_cell_and_exact_dat_timing() {
    let source = "graphics.drs:[32, 112, 108, 115]:4518".to_owned();
    let mut value = pack_fixture(
        REQUIRED_RENDER_SOURCES
            .iter()
            .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some()))
            .chain([(source.clone(), 60, false)]),
    );
    for record in value["frames"].as_array_mut().unwrap() {
        if record["source"] == source {
            record["anchor_x"] = record["frame"].clone();
        }
    }
    value["buildings"] = serde_json::json!([{"unit_id":45,"footprint":[3.0,3.0],
        "foundation_terrain":-1,"foundation_slp":null,"foundation_size":[0,0],
        "parts":[{"graphic_id":4411,"slp_id":4518,"source":source,"available":true,
            "frames":60,"angles":1,"primary":true,"replay_delay":0.0,"frame_rate":0.15,
            "offset":[0,0],"misplacement":[0.0,0.0],"layer":20}],"damage":[],"states":[],"construction":[]}]);
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load_collected().await.unwrap();
    drop(fetch);
    let wake = &art.compositions[0].parts[0];
    assert_eq!(wake.animation.len(), 60);
    assert_eq!(wake.frame_rate, 0.15);
    assert_eq!(wake.frame_rate * wake.animation.len() as f32, 9.0);
    assert_eq!(
        wake.animation
            .iter()
            .map(|f| f.anchor[0] as u32)
            .collect::<Vec<_>>(),
        (0..60).collect::<Vec<_>>()
    );
    // Odd cells are omitted from the atlas, but the imported prefix stays whole.
    value["frames"]
        .as_array_mut()
        .unwrap()
        .retain(|f| !(f["source"] == source && f["frame"] == 59));
    let fetch = FetchFixture::install(&value);
    assert!(load_collected().await.is_err());
    drop(fetch);
}

#[wasm_bindgen_test]
async fn building_metadata_selects_new_frames_and_keeps_offsets_foundation_and_order() {
    let source = "graphics.drs:[32, 112, 108, 115]:90000".to_owned();
    let sources = REQUIRED_RENDER_SOURCES
        .iter()
        .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some()))
        .chain([(source.clone(), 1, false)]);
    let mut value = pack_fixture(sources);
    value["buildings"] = serde_json::json!([{
        "unit_id":109, "footprint":[4.0,4.0], "foundation_terrain":27,
        "foundation_slp":15000, "foundation_size":[10,10], "parts":[{
            "graphic_id":10,"slp_id":90000,"source":source,
            "available":true,"frames":1,"angles":1,"primary":true,"replay_delay":0.0,"frame_rate":0.0,"offset":[-17,29],"misplacement":[1.0,-0.5],"layer":20
        }], "damage":[],"states":[],"construction":[]
    }]);
    for (slp, material) in [(15000, 2), (15006, 13)] {
        value["buildings"][0]["foundation_slp"] = slp.into();
        compile(&mut value);
        let fetch = FetchFixture::install(&value);
        let (art, _, _) = load_collected().await.expect("DAT-only source loads");
        drop(fetch);
        assert_eq!(art.compositions.len(), 1);
        assert_eq!(art.compositions[0].unit_id, 109);
        assert_eq!(art.compositions[0].foundation_material, Some(material));
        assert_eq!(art.compositions[0].parts[0].offset, [7.0, -7.0]);
        assert_eq!(art.compositions[0].parts[0].depth, -1.5);
    }
    value["buildings"][0]["parts"][0]["offset"] = serde_json::json!([i32::MIN, 0]);
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    assert!(
        load_collected()
            .await
            .err()
            .unwrap()
            .as_string()
            .unwrap()
            .contains("invalid building part offset")
    );
    drop(fetch);
}

#[wasm_bindgen_test]
async fn farm_foundation_loads_the_dat_sheet_in_the_terrain_domain() {
    let source = "terrain.drs:[32, 112, 108, 115]:15004".to_owned();
    let sources = REQUIRED_RENDER_SOURCES
        .iter()
        .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some()))
        .chain([(source.clone(), 100, true)]);
    let mut value = pack_fixture(sources);
    value["buildings"] = serde_json::json!([{"unit_id":50,"footprint":[3.0,3.0],"foundation_terrain":7,"foundation_slp":15004,"foundation_size":[10,10],"parts":[],"damage":[],"states":[],"construction":[]}]);
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load_collected().await.expect("DAT foundation sheet");
    drop(fetch);
    assert_eq!(art.compositions[0].foundation_material, Some(17));
    assert_eq!(art.foundation.0.len(), 100);
    assert!(art.foundation.0.iter().all(|f| f.atlas.page < 2));
}

#[wasm_bindgen_test]
async fn dat_variants_and_construction_angles_survive_runtime_selection() {
    let source = "graphics.drs:[32, 112, 108, 115]:90000".to_owned();
    let extra = "de/b_misc_foundation_2x2_x1.sld".to_owned();
    let mut value = pack_fixture(
        REQUIRED_RENDER_SOURCES
            .iter()
            .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some()))
            .chain([(source.clone(), 3, false), (extra, 3, false)]),
    );
    let part = serde_json::json!({"graphic_id":1,"slp_id":90000,"source":source,"available":true,"frames":1,"angles":3,"primary":true,"replay_delay":0.0,"frame_rate":2.0,"offset":[-9,11],"misplacement":[0.0,0.0],"layer":20});
    value["buildings"] = serde_json::json!([{"unit_id":70,"footprint":[2.0,2.0],"foundation_terrain":-1,"foundation_slp":null,"foundation_size":[0,0],"parts":[part.clone()],"damage":[],"states":[],"construction":[part]}]);
    value["buildings"][0]["parts"][0]["frames"] = 3.into();
    value["buildings"][0]["parts"][0]["angles"] = 1.into();
    value["buildings"][0]["parts"][0]["frame_rate"] = 0.into();
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load_collected()
        .await
        .expect("all source angles and variants load");
    drop(fetch);
    assert_eq!(art.compositions[0].parts[0].animation.len(), 3);
    assert_eq!(art.compositions[0].construction[0].animation.len(), 3);
    assert_eq!(art.compositions[0].construction[0].angles, 3);
    assert!(art.buildings[1].is_empty(), "DAT House owns completed art");
    assert!(
        art.buildings[10].is_empty(),
        "DAT construction owns its native stages"
    );
}

#[wasm_bindgen_test]
async fn native_wall_keeps_all_damage_cells_in_every_connectivity_angle() {
    let source = "graphics.drs:[32, 112, 108, 115]:90000".to_owned();
    let mut value = pack_fixture(
        REQUIRED_RENDER_SOURCES
            .iter()
            .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some()))
            .chain([(source.clone(), 45, false)]),
    );
    value["buildings"] = serde_json::json!([{
        "unit_id":72,"footprint":[1.0,1.0],"foundation_terrain":-1,
        "foundation_slp":null,"foundation_size":[0,0],"damage":[],"states":[],"construction":[],
        "parts":[{"graphic_id":1,"slp_id":90000,"source":source,"available":true,
            "frames":9,"angles":5,"primary":true,"replay_delay":0.0,"frame_rate":0.2,
            "offset":[-3,7],"misplacement":[0.0,0.0],"layer":20}]
    }]);
    for frame in value["frames"].as_array_mut().unwrap() {
        if frame["source"] == source {
            frame["width"] = (frame["frame"].as_u64().unwrap() + 1).into();
        }
    }
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load_collected()
        .await
        .expect("native wall orientations load");
    drop(fetch);
    let part = &art.compositions[0].parts[0];
    assert_eq!(part.angles, 5);
    assert_eq!(
        part.animation.len(),
        45,
        "all native cells remain addressable"
    );
    assert_eq!(
        part.animation
            .iter()
            .map(|frame| frame.size[0])
            .collect::<Vec<_>>(),
        (1..=45).map(|v| v as f32).collect::<Vec<_>>()
    );
    assert_eq!(part.offset, [-3.0, 7.0]);
    assert!(
        art.buildings[7].is_empty(),
        "native wall replaces supplemental art"
    );
}

#[wasm_bindgen_test]
async fn absent_feudal_ambient_parent_keeps_available_dat_body_and_deltas() {
    let source = "graphics.drs:[32, 112, 108, 115]:90000".to_owned();
    let mut value = pack_fixture(
        REQUIRED_RENDER_SOURCES
            .iter()
            .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some()))
            .chain([(source.clone(), 1, false)]),
    );
    let part = serde_json::json!({"graphic_id":228,"slp_id":90000,"source":source,
        "available":true,"frames":1,"angles":1,"primary":false,"replay_delay":0.0,"frame_rate":0.0,
        "offset":[-13,9],"misplacement":[0.0,0.0],"layer":20});
    let mut missing = part.clone();
    missing["source"] = "absent-parent".into();
    missing["available"] = false.into();
    missing["primary"] = true.into();
    value["buildings"] = serde_json::json!([{"unit_id":133,"footprint":[3.0,3.0],
        "foundation_terrain":-1,"foundation_slp":null,"foundation_size":[0,0],
        "parts":[missing,part],"damage":[],"states":[],"construction":[]}]);
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load_collected()
        .await
        .expect("available DAT bodies survive absent ambient art");
    drop(fetch);
    let dock = art.compositions.iter().find(|b| b.unit_id == 133).unwrap();
    assert_eq!(dock.parts.len(), 1);
    assert_eq!(dock.parts[0].offset, [-13.0, 9.0]);
    assert!(
        art.buildings[44].is_empty(),
        "a native body must not allocate a duplicate fallback"
    );
}

#[wasm_bindgen_test]
async fn native_dock_without_construction_keeps_the_optional_foundation() {
    let source = "graphics.drs:[32, 112, 108, 115]:90000".to_owned();
    let mut value = pack_fixture(
        REQUIRED_RENDER_SOURCES
            .iter()
            .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some()))
            .chain([
                (source.clone(), 1, false),
                ("de/b_misc_foundation_dock_x1.sld".into(), 3, false),
            ]),
    );
    value["buildings"] = serde_json::json!([{"unit_id":45,"footprint":[3.0,3.0],
        "foundation_terrain":-1,"foundation_slp":null,"foundation_size":[0,0],
        "parts":[{"graphic_id":1,"slp_id":90000,"source":source,"available":true,
            "frames":1,"angles":1,"primary":true,"replay_delay":0.0,"frame_rate":0.0,
            "offset":[0,0],"misplacement":[0.0,0.0],"layer":20}],"damage":[],"states":[],"construction":[]}]);
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load_collected()
        .await
        .expect("missing DAT construction retains optional art");
    drop(fetch);
    assert!(art.buildings[23].is_empty());
    assert_eq!(art.buildings[24].len(), 3);
}

#[wasm_bindgen_test]
async fn damage_graphic_instances_keep_native_offsets_and_shared_animation() {
    let source = "graphics.drs:[32, 112, 108, 115]:90000".to_owned();
    let mut value = pack_fixture(
        REQUIRED_RENDER_SOURCES
            .iter()
            .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some()))
            .chain([(source.clone(), 20, false)]),
    );
    let flame = serde_json::json!({"graphic_id":1,"slp_id":90000,"source":source,
        "available":true,"frames":20,"angles":1,"primary":false,"replay_delay":0.0,"frame_rate":0.05,
        "offset":[-9,11],"misplacement":[0.0,0.0],"layer":20});
    let mut second = flame.clone();
    second["offset"] = serde_json::json!([17, -3]);
    value["buildings"] = serde_json::json!([{"unit_id":70,"footprint":[2.0,2.0],
        "foundation_terrain":-1,"foundation_slp":null,"foundation_size":[0,0],
        "parts":[],"states":[],"construction":[],"damage":[{"damage_percent":25,"apply_mode":0,"parts":[flame,second]}]}]);
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load_collected().await.unwrap();
    drop(fetch);
    let damage = &art.compositions[0].damage[0];
    assert_eq!(damage.damage_percent, 25);
    assert_eq!(
        damage.parts.iter().map(|p| p.offset).collect::<Vec<_>>(),
        [[-9.0, 11.0], [17.0, -3.0]]
    );
    assert_eq!(damage.parts[0].animation.len(), 20);
    assert_eq!(damage.parts[0].frame_rate, 0.05);
    assert_eq!(
        damage.parts[0].animation[1].atlas,
        damage.parts[1].animation[1].atlas
    );
}

#[wasm_bindgen_test]
async fn building_overlay_replaces_recipes_and_offsets_every_native_state_record() {
    let source = "de/synthetic-building.sld";
    let mut extra = pack_fixture([(source.into(), 512, false)]);
    extra["chunks"] = serde_json::json!([{"file":"native.bin","hash":"a".repeat(64),"bytes":512}]);
    for f in extra["frames"].as_array_mut().unwrap() {
        f["native"] = serde_json::json!({"chunk":0,"offset":f["frame"],"length":1});
        f["x"] = 0.into();
        f["y"] = 0.into();
        f["page"] = 0.into();
    }
    let part = serde_json::json!({"graphic_id":7,"slp_id":-1,"source":source,"available":true,
        "frames":32,"angles":16,"primary":true,"frame_rate":0.03125,"replay_delay":0.125,
        "offset":[-4,9],"misplacement":[0.0,0.0],"layer":20});
    extra["buildings"] = serde_json::json!([{"unit_id":70,"footprint":[2.0,2.0],"foundation_terrain":-1,
        "foundation_slp":null,"foundation_size":[0,0],"parts":[part.clone()],"construction":[],"damage":[],
        "states":[{"role":0,"parts":[part.clone()]},{"role":1,"parts":[part]}]}]);
    compile(&mut extra);
    let _fetch = FetchFixture::install(&extra);
    js_sys::Function::new_with_args("data", "window.fetch=async()=>new Response(data)")
        .call1(&JsValue::NULL, &JsValue::from_str(&extra.to_string()))
        .unwrap();
    let mut manifest: Manifest = serde_json::from_value(manifest::fixture()).unwrap();
    let before = manifest.frames.len();
    building_sprites::overlay(&mut manifest).await.unwrap();
    assert_eq!(manifest.buildings[0].parts[0].record, Some(before));
    assert_eq!(manifest.buildings[0].states[1].1[0].record, Some(before));
    assert_eq!(manifest.chunks[0].file, "buildings/native.bin");
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    buildings::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(
        selected.len(),
        512,
        "every native cell is selected once, even when shared by lifecycle states"
    );
    let placements = placement::plan(&manifest, &selected, &domains).unwrap();
    assert_eq!(
        placement::pages(&placements),
        8,
        "native cells reserve no additional startup PNG pages"
    );
    let records = vec![
        GameFrame {
            stream: 0,
            atlas: aoe_rendering::AtlasAddress {
                page: 0,
                uv: [0.0; 4]
            },
            size: [1.0; 2],
            anchor: [0.0; 2]
        };
        selected.len()
    ];
    let art = buildings::resolve(&manifest, &selected, &records);
    assert_eq!(art[0].parts[0].animation.len(), 512);
    assert_eq!(art[0].parts[0].frame_rate, 0.03125);
    assert_eq!(art[0].parts[0].replay_delay, 0.125);
    assert_eq!(art[0].states[0].1[0].animation.len(), 512);
}
