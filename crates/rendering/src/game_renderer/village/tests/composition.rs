use super::tests::{art, camera, unit};
use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn placement_ghost_preserves_native_parts_and_tints_only_invalid_sites() {
    let art = art();
    let mut original = unit(SceneUnitKind::TownCenter);
    original.visual = original
        .visual
        .with_owner(Some([65, 145, 245]), false, false);
    let normal = super::super::world_sprite_frames(&art, &[], &[], &[original], camera(), 0);
    for valid in [true, false] {
        let ghost = SceneUnit {
            visual: original.visual.with_ghost(!valid).with_ghost(valid),
            ..original
        };
        let frames = super::super::world_sprite_frames(&art, &[], &[], &[ghost], camera(), 0);
        assert_eq!(frames.len(), normal.len());
        for ((sprite, frame, _, _), (body, native, _, _)) in frames.iter().zip(&normal) {
            assert_eq!(frame.atlas, native.atlas);
            assert_eq!(frame.anchor, native.anchor);
            assert_eq!(
                sprite.color,
                if valid {
                    [1.0, 1.0, 1.0, 0.5]
                } else {
                    [3.0, 0.2, 0.2, 0.5]
                }
            );
            assert_eq!(sprite.pages[3] & Sprite::OCCLUDER, 0);
            assert_eq!(sprite.pages[3] & Sprite::PLAYER_MASK, Sprite::PLAYER_MASK);
            assert_eq!(sprite.terrain_blend[1], body.terrain_blend[1]);
        }
    }
}

#[wasm_bindgen_test]
fn farm_ghost_draws_nine_native_crop_tiles_without_painting_ground() {
    let mut art = art();
    art.foundation.0 = vec![art.compositions[0].parts[0].frame];
    let farm = SceneUnit {
        visual: crate::SceneUnitVisual::new(false).with_ghost(true),
        ..unit(SceneUnitKind::Farm)
    };
    let frames = super::super::world_sprite_frames(&art, &[], &[], &[farm], camera(), 0);
    assert_eq!(frames.len(), 9);
    assert!(
        frames
            .iter()
            .all(|(s, f, _, _)| s.color[3] == 0.5 && f.atlas == art.foundation.0[0].atlas)
    );
    assert!(frames.iter().all(|(s, _, _, _)| s.pages[3] & 128 != 0));
    let mut ground = vec![SceneTerrain {
        position: farm.position,
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
        appearance: None,
    }];
    crate::apply_building_foundations(&mut ground, &art, &[farm]);
    assert_eq!(
        ground[0].material, 0,
        "previews never paint the cached terrain"
    );
}

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

#[wasm_bindgen_test]
fn native_building_parts_keep_owner_masks_and_a_hostile_red_selection() {
    let art = art();
    let mut building = unit(SceneUnitKind::TownCenter);
    building.visual = crate::SceneUnitVisual::new(false)
        .with_player(
            Some(aoe_core::PlayerId(2)),
            aoe_core::PlayerId(0),
            crate::PLAYER_COLOURS[0],
            false,
        )
        .with_selected(true);
    let layers =
        super::super::direct_world_layers(Vec::new(), &art, &[], &[], &[building], camera(), 0);
    let mut bodies = 0;
    let mut rings = 0;
    for layer in layers {
        match layer {
            super::super::WorldLayer::Sprite(sprite, _, _, _) => {
                assert_ne!(sprite.pages[3] & Sprite::PLAYER_MASK, 0);
                assert_eq!(
                    sprite.terrain_blend[1][..3],
                    crate::PLAYER_COLOURS[2].map(|c| f32::from(c) / 255.0)
                );
                bodies += 1;
            }
            super::super::WorldLayer::Selection(sprite, _) => {
                assert_eq!(
                    sprite.color,
                    [245.0 / 255.0, 65.0 / 255.0, 65.0 / 255.0, 1.0]
                );
                rings += 1;
            }
            _ => panic!("only native building parts and its ring expected"),
        }
    }
    assert_eq!(bodies, art.compositions[0].parts.len());
    assert_eq!(rings, 1);
}
#[wasm_bindgen_test]
fn bridge_scenario_pieces_use_independent_native_art_slots() {
    for slot in 65..71 {
        let kind = SceneUnitKind::building(slot);
        assert_eq!(kind.building_slot(), Some(slot));
        assert_eq!(kind.building_side(), Some(1.0));
        assert!(!kind.is_foundation());
        let mut deck = unit(kind);
        let mut actor = unit(SceneUnitKind::Villager);
        deck.position = [4.0, 4.0];
        actor.position = deck.position;
        assert!(super::super::scene_unit_depth(deck) < super::super::scene_unit_depth(actor));
    }
}

#[wasm_bindgen_test]
fn bridge_handrails_follow_the_span_axis() {
    // AoK A is 2x3 along Y; B is 3x2 along X. Rails follow that long axis.
    for (along_x, expected) in [
        (
            true,
            [
                SceneUnitKind::BridgeBTop,
                SceneUnitKind::BridgeBMiddle,
                SceneUnitKind::BridgeBBottom,
            ],
        ),
        (
            false,
            [
                SceneUnitKind::BridgeATop,
                SceneUnitKind::BridgeAMiddle,
                SceneUnitKind::BridgeABottom,
            ],
        ),
    ] {
        for (piece, kind) in expected.into_iter().enumerate() {
            assert_eq!(SceneUnitKind::bridge(along_x, piece as u8), kind);
        }
    }
}

#[wasm_bindgen_test]
fn shared_bridge_art_never_samples_an_unrelated_owner_mask_page() {
    let mut art = art();
    for slot in 65..71 {
        for page in 0..5 {
            let mut frame = art.village[0][0];
            frame.atlas.page = page;
            art.buildings[slot] = vec![frame];
            let mut deck = unit(SceneUnitKind::building(slot as u8));
            deck.visual = deck.visual.with_owner(Some([65, 145, 245]), false, true);
            let frames = super::super::world_sprite_frames(&art, &[], &[], &[deck], camera(), 0);
            assert_eq!(frames.len(), 1);
            let sprite = frames[0].0;
            assert_eq!(
                sprite.pages[3] & Sprite::PLAYER_MASK,
                0,
                "unowned bridge slot {slot} on colour page {page}"
            );
            assert_ne!(
                sprite.pages[3] & Sprite::UNIT_CONTOUR,
                0,
                "selection contour survives without owner recolouring"
            );
        }
    }
}

#[wasm_bindgen_test]
fn castle_buildings_clear_their_foundations_while_only_bridges_stay_below_actors() {
    let dark = unit(SceneUnitKind::BuildingTownCenter);
    let feudal = unit(SceneUnitKind::FeudalTownCenter);
    let expected = super::super::scene_unit_depth(dark);
    assert_eq!(super::super::scene_unit_depth(feudal), expected);
    for slot in 45..65 {
        let building = unit(SceneUnitKind::building(slot));
        assert_eq!(
            super::super::scene_unit_depth(building),
            expected,
            "Castle building slot {slot} must clear its own foundation like Dark Age art"
        );
    }
    let actor = unit(SceneUnitKind::Villager);
    for slot in 65..71 {
        let bridge = unit(SceneUnitKind::building(slot));
        assert!(super::super::scene_unit_depth(bridge) < super::super::scene_unit_depth(actor));
    }
}

#[wasm_bindgen_test]
fn damage_overlay_uses_selected_stage_and_animates() {
    let mut art = art();
    let mut flame = art.compositions[0].parts[1].clone();
    flame.animation = art.village[1][..2].to_vec();
    flame.angles = 1;
    flame.frame_rate = 0.1;
    art.compositions[0].damage = vec![crate::BuildingDamage {
        damage_percent: 25,
        apply_mode: 0,
        parts: vec![flame.clone()],
    }];
    let mut building = unit(SceneUnitKind::TownCenter);
    let healthy = super::super::world_sprite_frames(&art, &[], &[], &[building], camera(), 0);
    building.visual = building.visual.with_damage_stage(1);
    let burning = super::super::world_sprite_frames(&art, &[], &[], &[building], camera(), 0);
    let next = super::super::world_sprite_frames(&art, &[], &[], &[building], camera(), 1);
    assert_eq!(burning.len(), healthy.len() + 1);
    assert_eq!(burning.last().unwrap().1.atlas, flame.animation[0].atlas);
    assert_eq!(next.last().unwrap().1.atlas, flame.animation[1].atlas);
}

#[wasm_bindgen_test]
fn damage_stage_uses_exact_hp_thresholds_and_repairs() {
    let mut art = art();
    art.compositions[0].damage = [25, 50, 75]
        .into_iter()
        .map(|damage_percent| crate::BuildingDamage {
            damage_percent,
            apply_mode: 0,
            parts: Vec::new(),
        })
        .collect();
    let building = &art.compositions[0];
    assert_eq!(building.damage_stage(751, 1000), 0);
    assert_eq!(building.damage_stage(750, 1000), 1);
    assert_eq!(building.damage_stage(500, 1000), 2);
    assert_eq!(building.damage_stage(250, 1000), 3);
    assert_eq!(building.damage_stage(1000, 1000), 0);
    assert_eq!(building.damage_stage(0, 0), 0);
}
