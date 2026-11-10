use super::{art, camera, unit};
use crate::{SceneUnitKind, game_renderer::world_sprite_frames};
use aoe_core::EntityId;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn barrier_native_rows_follow_orientation_independently_of_entity_id() {
    for kind in [SceneUnitKind::PalisadeWall, SceneUnitKind::StoneWall] {
        let mut art = art();
        let mut part = art.compositions[0].parts[0].clone();
        part.angles = 5;
        part.animation = art.village[1][..10].to_vec();
        part.frame_rate = 0.0;
        art.compositions[0].unit_id = kind.building_id().unwrap();
        art.compositions[0].parts = vec![part.clone()];
        art.compositions[0].states = vec![(5, vec![part.clone()])];
        for facing in 0..5 {
            for id in [3, 17] {
                let actor = crate::SceneUnit {
                    id: EntityId(id),
                    facing,
                    ..unit(kind)
                };
                let draw = world_sprite_frames(&art, &[], &[], &[actor], camera(), 0);
                assert_eq!(
                    draw.last().unwrap().1.atlas,
                    part.animation[usize::from(facing) * 2].atlas,
                    "{kind:?}, facing={facing}, id={id}"
                );
            }
        }
    }
}

#[wasm_bindgen_test]
fn gates_use_complete_dat_assemblies_for_each_axis_and_state() {
    for (closed, open, ids) in [
        (
            SceneUnitKind::StoneGate,
            SceneUnitKind::OpenStoneGate,
            [490, 487],
        ),
        (
            SceneUnitKind::PalisadeGate,
            SceneUnitKind::OpenPalisadeGate,
            [796, 792],
        ),
    ] {
        let mut art = art();
        let mut recipes = super::art().compositions;
        art.compositions.clear();
        for (axis, id) in ids.into_iter().enumerate() {
            let mut recipe = recipes
                .pop()
                .unwrap_or_else(|| super::art().compositions.remove(0));
            recipe.unit_id = id;
            let mut shut = recipe.parts[0].clone();
            shut.animation.clear();
            shut.frame = art.village[1][axis * 2];
            let mut opened = shut.clone();
            opened.frame = art.village[1][axis * 2 + 1];
            recipe.parts = vec![shut];
            recipe.states = vec![(5, vec![opened])];
            art.compositions.push(recipe);
        }
        for (state, kind) in [closed, open].into_iter().enumerate() {
            for axis in 0..2 {
                let gate = crate::SceneUnit {
                    facing: axis as u8 * 16,
                    ..unit(kind)
                };
                let draw = world_sprite_frames(&art, &[], &[], &[gate], camera(), 0);
                assert_eq!(
                    draw.last().unwrap().1.atlas,
                    art.village[1][axis * 2 + state].atlas,
                    "{kind:?}, axis={axis}"
                );
            }
        }
    }
}
