use super::tests::{art, camera, unit};
use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

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
