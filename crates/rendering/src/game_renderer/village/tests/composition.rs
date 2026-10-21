use super::tests::{art, camera, unit};
use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn remembered_town_center_keeps_every_dat_part_and_native_anchor_in_fog() {
    let art = art();
    let units = [unit(SceneUnitKind::TownCenter)];
    let mut layers =
        super::super::direct_world_layers(Vec::new(), &art, &[], &[], &units, camera(), 0);
    let before = layers
        .iter()
        .map(|layer| match layer {
            super::super::WorldLayer::Sprite(sprite, frame, depth, id) => {
                (*sprite, *frame, *depth, *id)
            }
            _ => panic!("only DAT parts expected"),
        })
        .collect::<Vec<_>>();
    assert_eq!(before.len(), art.compositions[0].parts.len());
    super::super::fog::shade_objects(&mut layers, &art, &[], &units, 0, &|_| 0.45);
    for (layer, (sprite, frame, depth, id)) in layers.iter().zip(before) {
        let super::super::WorldLayer::Sprite(shaded, after_frame, after_depth, after_id) = layer
        else {
            panic!("DAT part lost in fog");
        };
        assert_eq!(after_frame.atlas, frame.atlas);
        assert_eq!(after_frame.anchor, frame.anchor);
        assert_eq!(after_frame.size, frame.size);
        assert_eq!(*after_depth, depth);
        assert_eq!(*after_id, id);
        assert_eq!(shaded.uv, sprite.uv);
        assert_eq!(shaded.color[..3], [0, 1, 2].map(|i| sprite.color[i] * 0.45));
    }
}

#[wasm_bindgen_test]
fn building_parts_use_pack_order_signed_pixels_and_annex_depth_without_mirroring() {
    let mut art = art();
    let frame = art.compositions[0].parts[0].frame;
    art.compositions[0].parts = vec![
        crate::BuildingPart {
            frame,
            animation: Vec::new(),
            angles: 1,
            frame_rate: 0.0,
            offset: [-31.0, 17.0],
            depth: -1.0,
        },
        crate::BuildingPart {
            frame,
            animation: Vec::new(),
            angles: 1,
            frame_rate: 0.0,
            offset: [11.0, -9.0],
            depth: 0.0,
        },
    ];
    let frames = super::super::world_sprite_frames(
        &art,
        &[],
        &[],
        &[unit(SceneUnitKind::TownCenter)],
        camera(),
        0,
    );
    assert_eq!(frames.len(), 2);
    let scale = aoe_core::ISO_TILE_WIDTH as f32 / 96.0;
    assert!((frames[0].1.anchor[0] - (frame.anchor[0] + 31.0) * scale).abs() < 0.001);
    assert!((frames[1].1.anchor[1] - (frame.anchor[1] + 9.0) * scale).abs() < 0.001);
    assert!(frames.iter().all(|(s, _, _, _)| s.uv[2] > 0.0));
}

#[wasm_bindgen_test]
fn an_unfinished_house_uses_its_construction_graphic() {
    let mut art = art();
    art.compositions[0].unit_id = 70;
    art.compositions[0].construction = vec![art.compositions[0].parts[1].clone()];
    let frames = super::super::world_sprite_frames(
        &art,
        &[],
        &[],
        &[unit(SceneUnitKind::foundation(70))],
        camera(),
        0,
    );
    assert_eq!(frames.len(), 1);
    assert_eq!(
        frames[0].1.atlas,
        art.compositions[0].construction[0].frame.atlas
    );
}

#[wasm_bindgen_test]
fn mill_animation_uses_dat_frame_duration_without_changing_delta_order() {
    let mut art = art();
    art.compositions[0].unit_id = 68;
    let first = art.compositions[0].parts[0].frame;
    let second = art.compositions[0].parts[1].frame;
    art.compositions[0].parts[0].animation = vec![first, second];
    art.compositions[0].parts[0].frame_rate = 0.2;
    let u = unit(SceneUnitKind::Mill);
    assert!(super::super::scene_unit_animates(&art, u, camera()));
    for (animation, expected) in [(0, first), (1, first), (2, second), (4, first)] {
        let frames = super::super::world_sprite_frames(&art, &[], &[], &[u], camera(), animation);
        assert_eq!(frames[0].1.atlas, expected.atlas);
        assert_eq!(
            frames[1].1.atlas, second.atlas,
            "delta stays after the animated stand graphic"
        );
    }
}

#[wasm_bindgen_test]
fn construction_stages_follow_dat_offsets_and_keep_the_building_identity() {
    let mut art = art();
    art.compositions[0].unit_id = 70;
    let mut part = art.compositions[0].parts[0].clone();
    part.offset = [-19.0, 23.0];
    part.angles = 3;
    part.animation = art.compositions[0].parts[..3]
        .iter()
        .map(|p| p.frame)
        .collect();
    let expected = part.animation[2];
    art.compositions[0].construction = vec![part];
    let mut house = unit(SceneUnitKind::foundation(70));
    house.facing = 2;
    let frames = super::super::world_sprite_frames(&art, &[], &[], &[house], camera(), 90);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].1.atlas, expected.atlas);
    let scale = aoe_core::ISO_TILE_WIDTH as f32 / 96.0;
    assert!((frames[0].1.anchor[0] - (expected.anchor[0] + 19.0) * scale).abs() < 0.001);
    assert_eq!(house.kind.building_id(), Some(70));
}

#[wasm_bindgen_test]
fn actor_and_building_tags_keep_their_selections_disjoint() {
    for slot in 0..45 {
        assert_eq!(SceneUnitKind::building(slot).building_slot(), Some(slot));
    }
    for slot in 0..50 {
        let actor = SceneUnitKind::actor(slot);
        assert_eq!(actor.actor_slot(), Some(usize::from(slot)));
        assert_eq!(actor.building_slot(), None);
        assert_eq!(actor.building_id(), None);
        assert!(!actor.is_foundation());
    }
    for id in [109, 70, 562, 584, 68, 50, 12, 72, 598, 45] {
        let foundation = SceneUnitKind::foundation(id);
        assert_eq!(foundation.actor_slot(), None);
        assert_eq!(foundation.building_id(), Some(i32::from(id)));
        assert!(foundation.is_foundation());
    }
    assert_eq!(SceneUnitKind::Dock.building_id(), Some(45));
    assert_eq!(SceneUnitKind::RubbleDock.building_id(), None);
}

#[wasm_bindgen_test]
fn feudal_families_compose_the_selected_dat_unit_and_construction() {
    for (slot, id) in [
        (26, 87),
        (27, 101),
        (28, 103),
        (29, 84),
        (30, 79),
        (38, 71),
        (39, 463),
        (40, 129),
        (41, 498),
        (44, 133),
    ] {
        let kind = SceneUnitKind::building(slot);
        assert_eq!(kind.building_id(), Some(id));
        let mut art = art();
        art.compositions[0].unit_id = id;
        let expected = art.compositions[0].parts.len();
        let frames = super::super::world_sprite_frames(&art, &[], &[], &[unit(kind)], camera(), 0);
        assert_eq!(
            frames.len(),
            expected,
            "every DAT delta is emitted for {id}"
        );
        let foundation = SceneUnitKind::foundation(id as i16);
        assert_eq!(foundation.building_id(), Some(id));
        assert!(foundation.is_foundation());
    }
}

#[wasm_bindgen_test]
fn feudal_mill_fallback_animates_when_native_parts_are_absent() {
    let mut art = art();
    let frame = art.compositions[0].parts[0].frame;
    art.compositions.clear();
    art.buildings[40] = vec![frame; 2];
    assert!(super::super::scene_unit_animates(
        &art,
        unit(SceneUnitKind::FeudalMill),
        camera()
    ));
}
