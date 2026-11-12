use super::*;

#[test]
fn manifest_checks_building_bounds_inventory_and_unique_units() {
    let hash = "a".repeat(64);
    let mut value = serde_json::json!({
        "version":1,"converter":"synthetic","input_hash":hash,"terrains":[],
        "chunks":[],"pages":[{"color":"c.png","player":"p.png","shadow":"s.png","outline":"o.png", "color_hash":hash,"player_hash":hash,"shadow_hash":hash,"outline_hash":hash,"width":1024,"height":1024}],
        "frames":[{"source":"synthetic","source_hash":hash,"frame":0,"page":0,"x":1,"y":1,"width":1,"height":1,"anchor_x":-5,"anchor_y":10}],
        "buildings":[{"unit_id":7,"footprint":[2.0,3.0],"foundation_terrain":-1,"foundation_slp":null,"foundation_size":[0,0],"damage":[],"states":[],"construction":[],"parts":[{"graphic_id":9,"slp_id":11,"source":"synthetic","available":true,"frames":1,"angles":1,"primary":true,"replay_delay":0.0,"frame_rate":0.0,"offset":[-32768,32768],"misplacement":[128.0,-128.0],"layer":20}]}]
    });
    let parse = |value: &serde_json::Value| {
        let mut value = value.clone();
        let buildings: Vec<crate::dat::BuildingComposition> =
            serde_json::from_value(value["buildings"].clone()).unwrap();
        let frames: Vec<FrameRecord> = serde_json::from_value(value["frames"].clone()).unwrap();
        value["building_data"] = super::super::recipes::encode(&buildings, &frames)
            .unwrap()
            .into();
        parse_manifest(&serde_json::to_vec(&value).unwrap())
    };
    assert!(
        parse(&value).is_ok(),
        "both signed displacement boundaries are valid"
    );
    for invalid in [
        serde_json::json!([i32::MIN, 0]),
        serde_json::json!([0, 32769]),
    ] {
        let mut bad = value.clone();
        bad["buildings"][0]["parts"][0]["offset"] = invalid;
        assert!(parse(&bad).is_err());
    }
    let mut bad = value.clone();
    bad["buildings"][0]["parts"][0]["misplacement"] = serde_json::json!([0.0, 128.01]);
    assert!(parse(&bad).is_err());
    bad = value.clone();
    bad["buildings"][0]["parts"][0]["available"] = false.into();
    assert!(
        parse(&bad).is_err(),
        "an absent resource cannot exist in the inventory"
    );
    bad = value.clone();
    bad["buildings"][0]["footprint"] = serde_json::json!([0.0, 2.0]);
    assert!(parse(&bad).is_err());
    let duplicate = value["buildings"][0].clone();
    value["buildings"].as_array_mut().unwrap().push(duplicate);
    assert!(parse(&value).is_err());
}

#[test]
fn repeated_gate_part_lists_fit_the_unchanged_recipe_cap() {
    use crate::dat::{BuildingComposition, BuildingPart};
    let part = BuildingPart {
        graphic_id: 1,
        slp_id: -1,
        source: "unavailable".into(),
        available: false,
        frames: 1,
        angles: 1,
        primary: true,
        frame_rate: 0.0,
        replay_delay: 0.0,
        offset: [-3, 7],
        misplacement: [1.0, -1.0],
        layer: 20,
    };
    let mut buildings: Vec<_> = (0..40)
        .map(|unit_id| BuildingComposition {
            unit_id,
            footprint: [4.0, 1.0],
            foundation_terrain: -1,
            foundation_slp: None,
            foundation_size: [0; 2],
            parts: vec![part.clone(); 32],
            construction: Vec::new(),
            states: Vec::new(),
            damage: Vec::new(),
        })
        .collect();
    let encoded = super::super::recipes::encode(&buildings, &[]).unwrap();
    assert_eq!(encoded.len() / 2, 1 + 40 * 28 + 32 * 33);
    assert!(encoded.len() / 2 < 4096, "inline parts alone exceed 32 KiB");
    for building in &mut buildings {
        for part in &mut building.parts {
            part.offset[0] = building.unit_id;
        }
    }
    assert!(
        super::super::recipes::encode(&buildings, &[])
            .unwrap_err()
            .to_string()
            .contains("exceed 32 KiB")
    );
}
