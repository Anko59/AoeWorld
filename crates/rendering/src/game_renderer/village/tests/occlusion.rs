use super::tests::{art, unit};
use super::*;
use aoe_core::EntityId;
use wasm_bindgen_test::wasm_bindgen_test;

fn art_with_archers() -> GameArt {
    let mut art = art();
    art.actors = vec![Vec::new(); 135];
    for (pose, size, anchor) in [
        (SceneUnitKind::ArcherStand, [20.0, 40.0], [10.0, 40.0]),
        // A tall decay frame reaches far below its hotspot, as the Scout's does.
        (SceneUnitKind::ArcherDie, [180.0, 160.0], [90.0, 120.0]),
    ] {
        art.actors[pose.actor_slot().unwrap()] = (0..16)
            .map(|i| GameFrame {
                stream: 0,
                atlas: crate::AtlasAddress::new(8, [i as f32 / 2048.0, 0.0, 0.01, 0.02]),
                size,
                anchor,
            })
            .collect();
    }
    art
}

fn camera() -> SceneCamera {
    SceneCamera {
        center: [10.5; 2],
        ..super::tests::camera()
    }
}

fn depths(frames: &[(Sprite, GameFrame, f64, u64)], id: u64) -> Vec<f64> {
    frames.iter().filter(|f| f.3 == id).map(|f| f.2).collect()
}

fn actor(kind: SceneUnitKind, id: u32, position: [f64; 2]) -> SceneUnit {
    let mut actor = unit(kind);
    actor.id = EntityId(id);
    actor.position = position;
    actor
}

#[wasm_bindgen_test]
fn an_actor_in_front_of_a_building_is_not_hidden_by_it() {
    let art = art_with_archers();
    let mut hall = unit(SceneUnitKind::TownCenter);
    hall.id = EntityId(1);
    hall.position = [10.5, 10.5];
    let hall_depth = super::super::scene_unit_depth(hall);
    // The footprint spans 8.5..12.5. Beside its right and left front faces the
    // actor's own depth is under the building's flat depth, though it is nearer.
    for position in [[12.8, 9.5], [9.5, 12.8], [12.8, 10.5], [13.0, 8.6]] {
        let walker = actor(SceneUnitKind::ArcherStand, 11, position);
        let frames =
            super::super::world_sprite_frames(&art, &[], &[], &[hall, walker], camera(), 0);
        let drawn = depths(&frames, 11);
        assert!(!drawn.is_empty());
        assert!(
            drawn.iter().all(|d| *d > hall_depth),
            "{position:?} hidden by the building: {drawn:?} vs {hall_depth}"
        );
    }
}

#[wasm_bindgen_test]
fn an_actor_behind_a_building_stays_behind_it() {
    let art = art_with_archers();
    let mut hall = unit(SceneUnitKind::TownCenter);
    hall.id = EntityId(1);
    hall.position = [10.5, 10.5];
    let hall_depth = super::super::scene_unit_depth(hall);
    for position in [[8.0, 10.5], [10.5, 8.0], [8.2, 12.0], [7.0, 7.0]] {
        let walker = actor(SceneUnitKind::ArcherStand, 11, position);
        let frames =
            super::super::world_sprite_frames(&art, &[], &[], &[hall, walker], camera(), 0);
        for depth in depths(&frames, 11) {
            assert!(depth < hall_depth, "{position:?} in front: {depth}");
            assert_eq!(depth, super::super::scene_unit_depth(walker));
        }
    }
}

#[wasm_bindgen_test]
fn a_corpse_lies_under_an_actor_standing_just_behind_it() {
    let art = art_with_archers();
    let corpse = actor(SceneUnitKind::ArcherDie, 20, [10.5, 10.5]);
    let standing = actor(SceneUnitKind::ArcherStand, 21, [10.3, 10.3]);
    let frames =
        super::super::world_sprite_frames(&art, &[], &[], &[corpse, standing], camera(), 0);
    let lying = depths(&frames, 20);
    let upright = depths(&frames, 21);
    assert!(!lying.is_empty() && !upright.is_empty());
    for (lying, upright) in lying.iter().zip(&upright) {
        assert!(upright > lying, "corpse {lying} hides the actor {upright}");
    }
    // Still above the flat ground it lies on.
    assert!(lying.iter().all(|d| *d > surface_depth([10.5, 10.5, 0.0])));
}

#[wasm_bindgen_test]
fn an_actor_away_from_a_corpse_keeps_its_ground_depth() {
    let art = art_with_archers();
    let corpse = actor(SceneUnitKind::ArcherDie, 20, [10.5, 10.5]);
    let far = actor(SceneUnitKind::ArcherStand, 21, [8.5, 8.5]);
    let frames = super::super::world_sprite_frames(&art, &[], &[], &[corpse, far], camera(), 0);
    for depth in depths(&frames, 21) {
        assert_eq!(depth, super::super::scene_unit_depth(far));
    }
}
