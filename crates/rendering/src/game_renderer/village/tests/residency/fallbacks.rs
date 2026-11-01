use super::*;

fn animation(art: &mut GameArt) -> crate::BuildingPart {
    let mut part = art.compositions[0].parts[0].clone();
    let mut first = part.frame;
    first.stream = 1;
    first.size = [100.0, 80.0];
    first.anchor = [50.0, 80.0];
    part.frame = first;
    part.animation = vec![
        first,
        GameFrame {
            stream: 2,
            size: [90.0, 70.0],
            anchor: [45.0, 75.0],
            ..first
        },
    ];
    part.angles = 1;
    part.frame_rate = 0.05;
    part.replay_delay = 0.0;
    part.offset = [-7.0, 11.0];
    art.resident = vec![
        Some(crate::AtlasAddress {
            page: 8,
            uv: [0.1, 0.2, 0.04, 0.03],
        }),
        None,
    ];
    part
}

#[wasm_bindgen_test]
fn cold_native_building_parts_keep_their_own_previous_geometry_and_dat_offset() {
    let mut art = art();
    let part = animation(&mut art);
    art.compositions[0].parts = vec![part];
    let u = unit(SceneUnitKind::TownCenter);
    let before = world_sprite_frames(&art, &[], &[], &[u], camera(), 0);
    assert_eq!(before.len(), 1);
    assert_eq!(crate::scene_native_frames(&art, &[u], camera(), 50), [2]);
    let after = world_sprite_frames(&art, &[], &[], &[u], camera(), 50);
    assert_eq!(after.len(), 1, "cold building layers must remain visible");
    assert_eq!(after[0].0.uv, before[0].0.uv);
    assert_eq!(after[0].0.position, before[0].0.position);
    assert_eq!(after[0].1.size, before[0].1.size);
    assert_eq!(after[0].1.anchor, before[0].1.anchor);
}

#[wasm_bindgen_test]
fn cold_native_environment_parts_keep_their_own_previous_geometry_and_dat_offset() {
    let mut art = art();
    let part = animation(&mut art);
    art.environment.push(crate::BuildingArt {
        unit_id: 349,
        footprint: [1.0; 2],
        foundation_material: None,
        parts: vec![part],
        construction: Vec::new(),
        states: Vec::new(),
        damage: Vec::new(),
    });
    let r = SceneResource {
        id: 55,
        position: [0.5; 2],
        elevation_meters: 0.0,
        kind: 1,
        visual_family: 0,
        visual_variant: 0,
        highlight: 0,
    };
    let before = world_sprite_frames(&art, &[], &[r], &[], camera(), 0);
    assert_eq!(before.len(), 1);
    assert_eq!(
        crate::scene_resource_native_frames(&art, &[r], camera(), 51),
        [2]
    );
    let after = world_sprite_frames(&art, &[], &[r], &[], camera(), 51);
    assert_eq!(
        after.len(),
        1,
        "cold environment layers must remain visible"
    );
    assert_eq!(after[0].0.uv, before[0].0.uv);
    assert_eq!(after[0].0.position, before[0].0.position);
    assert_eq!(after[0].1.size, before[0].1.size);
    assert_eq!(after[0].1.anchor, before[0].1.anchor);
}

#[wasm_bindgen_test]
fn native_substitutions_keep_raw_demand_and_pin_only_visible_resident_representations() {
    let mut art = art();
    let part = animation(&mut art);
    art.compositions[0].parts = vec![part];
    let building = unit(SceneUnitKind::TownCenter);
    assert_eq!(
        crate::scene_native_substitutes(&art, &[building], &[], camera(), 51),
        [(2, 1)]
    );
    assert_eq!(
        crate::scene_native_frames(&art, &[building], camera(), 51),
        [2]
    );
    art.resident[0] = None;
    assert_eq!(
        crate::scene_native_substitutes(&art, &[building], &[], camera(), 51),
        [(2, 0)]
    );
    let distant = SceneUnit {
        position: [10000.0; 2],
        ..building
    };
    assert!(crate::scene_native_substitutes(&art, &[distant], &[], camera(), 51).is_empty());
}

#[wasm_bindgen_test]
fn streamed_rally_flags_request_native_pixels_and_hold_the_previous_phase() {
    let mut art = art();
    let part = animation(&mut art);
    art.scenery.rally_flags = vec![part.animation[1]; 10];
    art.scenery.rally_flags[0] = part.animation[0];
    let flag = SceneResource {
        id: 55,
        position: [0.5; 2],
        elevation_meters: 0.0,
        kind: SceneResource::RALLY_FLAG,
        visual_family: 0,
        visual_variant: 1,
        highlight: 0,
    };
    assert_eq!(
        crate::scene_resource_native_frames(&art, &[flag], camera(), 0),
        [2]
    );
    assert_eq!(
        crate::scene_native_substitutes(&art, &[], &[flag], camera(), 0),
        [(2, 1)]
    );
    let shown = world_sprite_frames(&art, &[], &[flag], &[], camera(), 0);
    let body = shown
        .iter()
        .find(|(sprite, _, _, _)| sprite.color[3] == 1.0)
        .unwrap();
    assert_eq!(body.1.stream, 1);
    assert_eq!(body.1.atlas, art.resident[0].unwrap());
}
