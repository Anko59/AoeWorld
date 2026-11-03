use super::*;

#[wasm_bindgen_test]
fn building_animation_keeps_frame_order_and_first_selected_alias() {
    let source = "synthetic-building";
    let mut value = pack_fixture([(source.into(), 3, false), ("unrelated".into(), 1, false)]);
    let part = serde_json::json!({"graphic_id":1,"slp_id":1,"source":source,
        "available":true,"frames":3,"angles":1,"primary":true,"frame_rate":0.1,
        "replay_delay":0.0,"offset":[0,0],"misplacement":[0.0,0.0],"layer":20});
    value["buildings"] = serde_json::json!([{"unit_id":70,"footprint":[2.0,2.0],
        "foundation_terrain":-1,"foundation_slp":null,"foundation_size":[0,0],
        "parts":[part],"construction":[],"damage":[],"states":[]}]);
    compile(&mut value);
    let mut manifest: Manifest = serde_json::from_value(value).unwrap();
    manifest.buildings = recipes::decode(&manifest.building_data, &manifest.frames).unwrap();
    let selected = [2, 3, 0, 1, 0];
    let records: Vec<_> = selected
        .iter()
        .enumerate()
        .map(|(slot, &index)| {
            let mut frame = placement::record(&manifest.frames[index], Placement::default());
            frame.anchor[0] = slot as f32;
            frame
        })
        .collect();
    let art = buildings::resolve(&manifest, &selected, &records);
    assert_eq!(art[0].parts[0].frame.anchor[0], 2.0);
    assert_eq!(
        art[0].parts[0]
            .animation
            .iter()
            .map(|f| f.anchor[0])
            .collect::<Vec<_>>(),
        [2.0, 3.0, 0.0]
    );
}

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

#[wasm_bindgen_test]
async fn overlay_without_drawable_fire_keeps_the_base_damage_animation() {
    let mut base = pack_fixture([("synthetic-fire".into(), 2, false)]);
    let part = serde_json::json!({"graphic_id":1,"slp_id":1,"source":"synthetic-fire",
        "available":true,"frames":2,"angles":1,"primary":false,"frame_rate":0.05,
        "replay_delay":0.0,"offset":[-12,-40],"misplacement":[0.0,0.0],"layer":20});
    let recipe = |parts: serde_json::Value| {
        serde_json::json!({"unit_id":70,"footprint":[2.0,2.0],
        "foundation_terrain":-1,"foundation_slp":null,"foundation_size":[0,0],"parts":[],
        "construction":[],"states":[],"damage":[{"damage_percent":25,"apply_mode":0,"parts":parts}]})
    };
    base["buildings"] = serde_json::json!([recipe(serde_json::json!([part]))]);
    compile(&mut base);
    let mut extra = pack_fixture([]);
    extra["buildings"] = serde_json::json!([recipe(serde_json::json!([]))]);
    compile(&mut extra);
    let _fetch = FetchFixture::install(&extra);
    js_sys::Function::new_with_args("data", "window.fetch=async()=>new Response(data)")
        .call1(&JsValue::NULL, &JsValue::from_str(&extra.to_string()))
        .unwrap();
    let mut manifest: Manifest = serde_json::from_value(base).unwrap();
    manifest.buildings = recipes::decode(&manifest.building_data, &manifest.frames).unwrap();
    building_sprites::overlay(&mut manifest).await.unwrap();
    let damage = &manifest.buildings[0].damage[0];
    assert_eq!(
        damage.parts.len(),
        1,
        "a particle-only overlay must retain the drawable base flame"
    );
    assert_eq!(damage.parts[0].record, Some(0));
    assert_eq!(damage.parts[0].offset, [-12, -40]);
    assert_eq!(damage.parts[0].frames, 2);
}

#[wasm_bindgen_test]
fn imperial_buildings_keep_castle_art_only_when_the_imperial_recipe_is_absent() {
    use aoe_protocol::{Age, BuildingKind};
    for kind in BuildingKind::ALL.into_iter().take(20) {
        let identity = |age| {
            aoe_rendering::SceneUnitKind::building(crate::building_ui::age_sprite_slot(
                kind, 1000, false, age,
            ) as u8)
            .building_id()
            .unwrap()
        };
        let (castle, imperial) = (identity(Age::Castle), identity(Age::Imperial));
        if castle == imperial {
            continue;
        }
        let mut value = pack_fixture([("castle".into(), 1, false), ("imperial".into(), 1, false)]);
        let recipe = |id, source| {
            let part = serde_json::json!({"graphic_id":1,"slp_id":1,"source":source,
                "available":true,"frames":1,"angles":1,"primary":true,"frame_rate":0.0,
                "replay_delay":0.0,"offset":[-9,11],"misplacement":[0.0,0.0],"layer":20});
            serde_json::json!({"unit_id":id,"footprint":[4.0,4.0],"foundation_terrain":-1,
                "foundation_slp":null,"foundation_size":[0,0],"parts":[part.clone()],
                "construction":[part.clone()],"damage":[],"states":[{"role":1,"parts":[part]}]})
        };
        for native in [false, true] {
            value["buildings"] = serde_json::json!([recipe(castle, "castle")]);
            if native {
                value["buildings"]
                    .as_array_mut()
                    .unwrap()
                    .push(recipe(imperial, "imperial"));
            }
            compile(&mut value);
            let mut manifest: Manifest = serde_json::from_value(value.clone()).unwrap();
            manifest.buildings =
                recipes::decode(&manifest.building_data, &manifest.frames).unwrap();
            let (mut selected, mut domains) = (Vec::new(), Vec::new());
            buildings::select(&manifest, &mut selected, &mut domains).unwrap();
            let records: Vec<_> = selected
                .iter()
                .map(|&i| {
                    let mut frame = placement::record(&manifest.frames[i], Placement::default());
                    frame.anchor[0] = i as f32;
                    frame
                })
                .collect();
            let art = buildings::resolve(&manifest, &selected, &records);
            let building = art
                .iter()
                .find(|b| b.unit_id == imperial)
                .expect("Imperial art survives age-up");
            assert_eq!(
                building.parts[0].frame.anchor[0],
                f32::from(native),
                "{kind:?}"
            );
            assert_eq!(building.construction.len(), 1);
            assert_eq!(building.states[0].1.len(), 1);
            assert_eq!(
                selected.len(),
                if native { 2 } else { 1 },
                "fallback reuses Castle cells"
            );
        }
    }
}
