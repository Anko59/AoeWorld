use super::tests::{art, camera, unit};
use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn siege_stone_draws_its_last_native_cell_from_one_animated_row() {
    let mut art = art();
    art.actors = vec![Vec::new(); 135];
    let kind = SceneUnitKind::Stone;
    let slot = kind.actor_slot().unwrap();
    art.actors[slot] = (0..30)
        .map(|i| GameFrame {
            atlas: crate::AtlasAddress::new(8, [i as f32 / 2048.0, 0.0, 0.01, 0.02]),
            size: [20.0, 20.0],
            anchor: [10.0, 20.0],
        })
        .collect();
    let u = unit(kind).with_actor_frame(29 << 3);
    let (_, stride, _) = frames(&art, u).unwrap();
    assert_eq!(stride, 30);
    let draw = super::super::world_sprite_frames(&art, &[], &[], &[u], camera(), 0);
    assert_eq!(draw.last().unwrap().1.atlas, art.actors[slot][29].atlas);
}

#[wasm_bindgen_test]
fn general_tags_draw_native_hero_cells_including_phases_above_31() {
    let mut art = art();
    art.actors = vec![Vec::new(); 135];
    for slot in 130..135 {
        let stride = if matches!(slot, 131 | 134) { 30 } else { 45 };
        art.actors[slot] = (0..16 * stride)
            .map(|i| GameFrame {
                atlas: crate::AtlasAddress::new(8, [i as f32 / 2048.0, 0.0, 0.01, 0.02]),
                size: [20.0, 40.0],
                anchor: [10.0, 40.0],
            })
            .collect();
        let kind = SceneUnitKind::actor(slot as u8);
        assert_eq!(kind.actor_slot(), Some(slot));
        let phase = stride - 1;
        for row in 0..16 {
            let facing = row as u16;
            let mut u = unit(kind);
            u = u.with_actor_frame(facing | (phase as u16) << 4);
            let (_, actual_stride, _) = frames(&art, u).unwrap();
            assert_eq!(actual_stride, stride);
            let (index, mirrored) = super::super::unit_frame_index(u, stride, phase);
            assert_eq!((index, mirrored), (row * stride + phase, false));
            let draw = super::super::world_sprite_frames(&art, &[], &[], &[u], camera(), 0);
            assert_eq!(draw.last().unwrap().1.atlas, art.actors[slot][index].atlas);
            assert!(super::super::scene_unit_bounds(&art, u, camera(), 0).is_some());
        }
    }
}

#[wasm_bindgen_test]
fn packed_actor_phase_preserves_scene_size_owner_selection_and_ghost_flags() {
    assert_eq!(std::mem::size_of::<SceneUnit>(), 32);
    let visual = crate::SceneUnitVisual::new(true)
        .with_owner(Some([65, 145, 245]), true, true)
        .with_selected(true);
    for frame in 0..720 {
        let mut u = unit(SceneUnitKind::GeneralAttack);
        u.visual = visual;
        u = u.with_actor_frame(frame);
        assert_eq!(u.actor_frame(), frame);
        assert_eq!(u.visual.colour(), visual.colour());
        assert!(u.visual.moving() && u.visual.selected() && u.visual.hostile());
        assert!(!u.visual.ghost() && !u.visual.invalid_ghost());
    }
}
