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
async fn non_gameplay_buildings_do_not_allocate_the_gameplay_atlas() {
    let mut value = pack_fixture(
        REQUIRED_RENDER_SOURCES
            .iter()
            .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some())),
    );
    // Dock exists in the DAT table, but the gameplay protocol has no Dock.
    value["buildings"] = serde_json::json!([{"unit_id":45,"footprint":[3.0,3.0],"foundation_terrain":-1,"foundation_slp":null,"foundation_size":[0,0],"parts":[{"graphic_id":1,"slp_id":99999,"source":"absent.slp","available":false,"frames":0,"angles":1,"primary":true,"frame_rate":0.0,"offset":[0,0],"misplacement":[0.0,0.0],"layer":20}],"construction":[]}]);
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load().await.expect("only gameplay recipes are resident");
    drop(fetch);
    assert!(art.compositions.is_empty());
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
            "available":true,"frames":1,"angles":1,"primary":true,"frame_rate":0.0,"offset":[-17,29],"misplacement":[1.0,-0.5],"layer":20
        }], "construction":[]
    }]);
    for (slp, material) in [(15000, 2), (15006, 13)] {
        value["buildings"][0]["foundation_slp"] = slp.into();
        compile(&mut value);
        let fetch = FetchFixture::install(&value);
        let (art, _, _) = load().await.expect("DAT-only source loads");
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
        load()
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
    value["buildings"] = serde_json::json!([{"unit_id":50,"footprint":[3.0,3.0],"foundation_terrain":7,"foundation_slp":15004,"foundation_size":[10,10],"parts":[],"construction":[]}]);
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load().await.expect("DAT foundation sheet");
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
    let part = serde_json::json!({"graphic_id":1,"slp_id":90000,"source":source,"available":true,"frames":1,"angles":3,"primary":true,"frame_rate":2.0,"offset":[-9,11],"misplacement":[0.0,0.0],"layer":20});
    value["buildings"] = serde_json::json!([{"unit_id":70,"footprint":[2.0,2.0],"foundation_terrain":-1,"foundation_slp":null,"foundation_size":[0,0],"parts":[part.clone()],"construction":[part]}]);
    value["buildings"][0]["parts"][0]["frames"] = 3.into();
    value["buildings"][0]["parts"][0]["angles"] = 1.into();
    value["buildings"][0]["parts"][0]["frame_rate"] = 0.into();
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load().await.expect("all source angles and variants load");
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
async fn native_wall_keeps_one_displayed_frame_per_connectivity_angle() {
    let source = "graphics.drs:[32, 112, 108, 115]:90000".to_owned();
    let mut value = pack_fixture(
        REQUIRED_RENDER_SOURCES
            .iter()
            .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some()))
            .chain([(source.clone(), 45, false)]),
    );
    value["buildings"] = serde_json::json!([{
        "unit_id":72,"footprint":[1.0,1.0],"foundation_terrain":-1,
        "foundation_slp":null,"foundation_size":[0,0],"construction":[],
        "parts":[{"graphic_id":1,"slp_id":90000,"source":source,"available":true,
            "frames":9,"angles":5,"primary":true,"frame_rate":0.2,
            "offset":[-3,7],"misplacement":[0.0,0.0],"layer":20}]
    }]);
    for frame in value["frames"].as_array_mut().unwrap() {
        if frame["source"] == source {
            frame["width"] = (frame["frame"].as_u64().unwrap() + 1).into();
        }
    }
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load().await.expect("native wall orientations load");
    drop(fetch);
    let part = &art.compositions[0].parts[0];
    assert_eq!(part.angles, 5);
    assert_eq!(
        part.animation.len(),
        5,
        "unused damage frames are not resident"
    );
    assert_eq!(
        part.animation
            .iter()
            .map(|frame| frame.size[0])
            .collect::<Vec<_>>(),
        [1.0, 10.0, 19.0, 28.0, 37.0]
    );
    assert_eq!(part.offset, [-3.0, 7.0]);
    assert!(
        art.buildings[7].is_empty(),
        "native wall replaces supplemental art"
    );
}
