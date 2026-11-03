use super::*;

#[wasm_bindgen_test]
fn actor_and_world_categories_rebase_independently_without_erasing_recipes() {
    let mut base: Manifest = serde_json::from_value(manifest::fixture()).unwrap();
    let mut world = pack_fixture([("world".into(), 1, false)]);
    world["chunks"] = serde_json::json!([{"file":"world.bin","hash":"a".repeat(64),"bytes":1}]);
    world["frames"][0]["native"] = serde_json::json!({"chunk":0,"offset":0,"length":1});
    world["buildings"] = serde_json::json!([{"unit_id":70,"footprint":[2.0,2.0],"foundation_terrain":-1,
        "foundation_slp":null,"foundation_size":[0,0],"construction":[],"damage":[],"states":[],
        "parts":[{"graphic_id":1,"slp_id":1,"source":"world","available":true,"frames":1,"angles":1,
            "primary":true,"frame_rate":0.0,"replay_delay":0.0,"offset":[0,0],"misplacement":[0.0,0.0],"layer":20}]}]);
    compile(&mut world);
    let mut actors = world.clone();
    actors["frames"][0]["source"] = "actor".into();
    actors["building_data"] = "00".into();
    actors["buildings"] = serde_json::json!([]);
    let before = base.frames.len();
    building_sprites::merge(
        &mut base,
        serde_json::from_value(world).unwrap(),
        "buildings",
    )
    .unwrap();
    building_sprites::merge(
        &mut base,
        serde_json::from_value(actors).unwrap(),
        "buildings/actors",
    )
    .unwrap();
    assert_eq!(base.frames.len(), before + 2);
    assert_eq!(base.buildings.len(), 1);
    assert_eq!(base.buildings[0].parts[0].record, Some(before));
    assert_eq!(base.chunks[0].file, "buildings/world.bin");
    assert_eq!(base.chunks[1].file, "buildings/actors/world.bin");
    assert_eq!(base.frames[before].native.as_ref().unwrap().chunk, 0);
    assert_eq!(base.frames[before + 1].native.as_ref().unwrap().chunk, 1);
}

#[wasm_bindgen_test]
fn unrendered_building_states_stay_validated_without_entering_residency() {
    let mut value = pack_fixture([("visible".into(), 1, false), ("snow".into(), 1, false)]);
    let part = |source| {
        serde_json::json!({"graphic_id":1,"slp_id":1,"source":source,
        "available":true,"frames":1,"angles":1,"primary":true,"frame_rate":0.0,"replay_delay":0.0,
        "offset":[0,0],"misplacement":[0.0,0.0],"layer":20})
    };
    value["buildings"] = serde_json::json!([{"unit_id":70,"footprint":[2.0,2.0],"foundation_terrain":-1,
        "foundation_slp":null,"foundation_size":[0,0],"parts":[part("visible")],"construction":[],
        "damage":[],"states":[{"role":2,"parts":[part("snow")]}]}]);
    compile(&mut value);
    let mut manifest = decode_manifest(&serde_json::to_vec(&value).unwrap()).unwrap();
    manifest.buildings = recipes::decode(&manifest.building_data, &manifest.frames).unwrap();
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    buildings::select(&manifest, &mut selected, &mut domains).unwrap();
    assert_eq!(selected, [0]);
    assert_eq!(
        manifest.buildings[0].states.len(),
        1,
        "source metadata is retained"
    );
    manifest.buildings[0].states[0].1[0].frames = 0;
    assert!(buildings::select(&manifest, &mut Vec::new(), &mut Vec::new()).is_err());
}
