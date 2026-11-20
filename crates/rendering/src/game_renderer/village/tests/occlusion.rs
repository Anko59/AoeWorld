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
    let hall_depth = super::super::scene_unit_depth(&art, hall);
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
    let hall_depth = super::super::scene_unit_depth(&art, hall);
    for position in [[8.0, 10.5], [10.5, 8.0], [8.2, 12.0], [7.0, 7.0]] {
        let walker = actor(SceneUnitKind::ArcherStand, 11, position);
        let frames =
            super::super::world_sprite_frames(&art, &[], &[], &[hall, walker], camera(), 0);
        for depth in depths(&frames, 11) {
            assert!(depth < hall_depth, "{position:?} in front: {depth}");
            assert_eq!(depth, super::super::scene_unit_depth(&art, walker));
        }
    }
}

#[wasm_bindgen_test]
fn a_rectangular_footprint_orders_actors_at_its_short_edge() {
    let mut art = art_with_archers();
    art.compositions[0].footprint = [2.0, 4.0];
    let hall = actor(SceneUnitKind::TownCenter, 1, [10.5; 2]);
    let front = actor(SceneUnitKind::ArcherStand, 11, [11.6, 9.0]);
    let back = actor(SceneUnitKind::ArcherStand, 12, [11.3, 11.8]);
    let frames =
        super::super::world_sprite_frames(&art, &[], &[], &[hall, front, back], camera(), 0);
    let hall = depths(&frames, 1);
    let (front, back) = (depths(&frames, 11), depths(&frames, 12));
    assert!(!hall.is_empty() && !front.is_empty() && !back.is_empty());
    assert!((24.0..24.1).contains(&hall[0]));
    assert!(front.iter().all(|d| *d > hall[0]));
    assert!(back.iter().all(|d| *d < hall[0]));
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
        assert_eq!(depth, super::super::scene_unit_depth(&art, far));
    }
}

#[wasm_bindgen_test]
fn destruction_and_ruins_keep_the_footprint_order() {
    let mut art = art_with_archers();
    let mut collapse = art.compositions[0].parts[0].clone();
    collapse.animation = art.village[1][..2].to_vec();
    collapse.frame_rate = 0.1;
    let collapse_uv = collapse.animation[0].atlas.uv;
    let ruin = art.compositions[0].parts[1].clone();
    let ruin_uv = ruin.frame.atlas.uv;
    art.compositions[0].states = vec![(0, vec![collapse]), (1, vec![ruin])];
    for (kind, collapsing, expected_uv) in [
        (SceneUnitKind::TownCenter, true, collapse_uv),
        (SceneUnitKind::RubbleTownCenter, false, ruin_uv),
    ] {
        let mut building = actor(kind, 1, [10.5; 2]);
        building.visual = crate::SceneUnitVisual::new(collapsing);
        let front = actor(SceneUnitKind::ArcherStand, 11, [12.8, 9.5]);
        let back = actor(SceneUnitKind::ArcherStand, 12, [9.5, 8.0]);
        let frames = super::super::world_sprite_frames(
            &art,
            &[],
            &[],
            &[building, front, back],
            camera(),
            0,
        );
        let base = surface_depth([12.5, 12.5, 0.0]);
        let building = depths(&frames, 1);
        assert_eq!(building.len(), 1);
        assert!(
            frames
                .iter()
                .any(|f| f.3 == 1 && f.1.atlas.uv == expected_uv)
        );
        assert!(building.iter().all(|d| (base..base + 0.1).contains(d)));
        let (front, back) = (depths(&frames, 11), depths(&frames, 12));
        assert!(!front.is_empty() && !back.is_empty());
        assert!(front.iter().all(|d| *d > building[0]));
        assert!(back.iter().all(|d| *d < building[0]));
    }
}

#[wasm_bindgen_test]
fn rock_depth_uses_the_ground_rim_even_with_a_tall_sprite() {
    let mut art = art_with_archers();
    let mut frame = art.village[1][0];
    frame.size = [200.0, 300.0];
    frame.anchor = [95.0, 250.0];
    art.scenery.rocks = vec![frame; 7];
    let rock = SceneResource {
        position: [10.5; 2],
        id: 100,
        kind: SceneResource::ROCK,
        visual_variant: 0,
        visual_family: 0,
        elevation_meters: 0.0,
        highlight: 0,
    };
    let front = actor(SceneUnitKind::ArcherStand, 11, [11.8, 10.5]);
    let back = actor(SceneUnitKind::ArcherStand, 12, [9.0, 10.5]);
    let frames = super::super::world_sprite_frames(&art, &[], &[rock], &[front, back], camera(), 0);
    let rock_depths = depths(&frames, 100);
    assert!(!rock_depths.is_empty());
    assert!((22.9..23.0).contains(&rock_depths[0]));
    assert!(depths(&frames, 11).iter().all(|d| *d > rock_depths[0]));
    assert!(depths(&frames, 12).iter().all(|d| *d < rock_depths[0]));
    art.scenery.rocks[0].anchor[1] -= 30.0;
    let changed = super::super::world_sprite_frames(&art, &[], &[rock], &[], camera(), 0);
    assert_eq!(depths(&changed, 100), rock_depths);
}

#[wasm_bindgen_test]
fn a_native_rock_hides_an_actor_behind_its_near_rim() {
    let mut art = art_with_archers();
    let mut rock_art = art.compositions.pop().unwrap();
    rock_art.unit_id = 623;
    art.environment.push(rock_art);
    let rock = SceneResource {
        position: [10.5; 2],
        id: 100,
        kind: SceneResource::ROCK,
        visual_variant: 0,
        visual_family: 0,
        elevation_meters: 0.0,
        highlight: 0,
    };
    let near = actor(SceneUnitKind::ArcherStand, 11, [11.8, 10.5]);
    let behind = actor(SceneUnitKind::ArcherStand, 12, [10.6, 10.95]);
    let frames =
        super::super::world_sprite_frames(&art, &[], &[rock], &[near, behind], camera(), 0);
    let rock = depths(&frames, 100);
    let (near, behind) = (depths(&frames, 11), depths(&frames, 12));
    assert!(!rock.is_empty() && !near.is_empty() && !behind.is_empty());
    assert!(near.iter().all(|d| *d > rock[0]));
    assert!(behind.iter().all(|d| *d < rock[0]));
}

#[wasm_bindgen_test]
fn contour_coverage_includes_imperial_buildings_and_excludes_decks() {
    let mut art = art();
    for (kind, expected) in [
        (SceneUnitKind::ImperialTownCenter, true),
        (SceneUnitKind::ImperialHouse, true),
        (SceneUnitKind::StoneWall, true),
        (SceneUnitKind::bridge(true, 1), false),
    ] {
        let slot = usize::from(kind.building_slot().unwrap());
        art.buildings[slot] = vec![art.village[1][0]];
        let building = actor(kind, 1, [10.5; 2]);
        let frames = super::super::world_sprite_frames(&art, &[], &[], &[building], camera(), 0);
        assert_eq!(frames.len(), 1);
        assert_eq!(
            frames[0].0.pages[3] & Sprite::OCCLUDER != 0,
            expected,
            "{kind:?}"
        );
    }
}
