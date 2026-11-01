use super::*;

#[wasm_bindgen_test]
async fn converted_categories_do_not_guess_sources_from_superseded_startup_lists() {
    let value = pack_fixture(
        REQUIRED_RENDER_SOURCES
            .iter()
            .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some()))
            .chain([
                ("graphics.drs:[32, 112, 108, 115]:2223".into(), 3, false),
                ("graphics.drs:[32, 112, 108, 115]:1911".into(), 49, false),
            ]),
    );
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load_collected().await.unwrap();
    drop(fetch);
    assert!(
        art.buildings.iter().all(Vec::is_empty),
        "only DAT recipes select building sources"
    );
    assert!(
        art.fishing.iter().all(Vec::is_empty),
        "resource fish use DAT environment recipes"
    );
}

#[wasm_bindgen_test]
async fn dat_building_and_corpse_geometry_keep_signed_anchors_and_half_scale_bridges() {
    let ids = [82, 104, 109, 605, 606, 607, 608, 609, 610];
    let extras = ids.into_iter().flat_map(|id| {
        [
            (format!("de/synthetic_{id}.sld"), 1, false),
            (format!("de/synthetic_{id}_corpse.sld"), 1, false),
        ]
    });
    let mut value = pack_fixture(
        REQUIRED_RENDER_SOURCES
            .iter()
            .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some()))
            .chain(extras),
    );
    for f in value["frames"].as_array_mut().unwrap() {
        f["anchor_x"] = (-3).into();
        f["anchor_y"] = 7.into();
    }
    value["buildings"] = ids
        .into_iter()
        .map(|id| {
            let part = |suffix: &str| {
                serde_json::json!({
            "graphic_id":1,"slp_id":90000,"source":format!("de/synthetic_{id}{suffix}.sld"),
            "available":true,"frames":1,"angles":1,"primary":true,"replay_delay":0.0,
            "frame_rate":0.0,"offset":[0,0],"misplacement":[0.0,0.0],"layer":20})
            };
            serde_json::json!({"unit_id":id,"footprint":[1.0,1.0],"foundation_terrain":-1,
            "foundation_slp":null,"foundation_size":[0,0],"parts":[part("")],
            "damage":[],"construction":[],"states":[{"role":1,"parts":[part("_corpse")]}]})
        })
        .collect::<Vec<_>>()
        .into();
    compile(&mut value);
    let fetch = FetchFixture::install(&value);
    let (art, _, _) = load_collected().await.unwrap();
    drop(fetch);
    for id in ids {
        let c = art.compositions.iter().find(|c| c.unit_id == id).unwrap();
        let scale = if (605..=610).contains(&id) { 0.5 } else { 1.0 };
        for p in c.parts.iter().chain(&c.states[0].1) {
            assert_eq!(p.frame.size, [scale; 2], "unit {id}");
            assert_eq!(p.frame.anchor, [-3.0 * scale, 7.0 * scale], "unit {id}");
        }
    }
}
