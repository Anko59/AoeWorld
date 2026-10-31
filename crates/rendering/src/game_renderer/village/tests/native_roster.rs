use super::*;
fn sequence(art: &GameArt, count: usize) -> Vec<GameFrame> {
    (0..count)
        .map(|i| {
            let mut f = art.village[1][0];
            f.atlas.uv[0] = i as f32 / 4096.0;
            f
        })
        .collect()
}
#[wasm_bindgen_test]
fn navy_and_workers_preserve_every_native_direction_and_full_phase() {
    let mut art = art();
    art.actors = vec![Vec::new(); 167];
    for kind in [
        SceneUnitKind::Galley,
        SceneUnitKind::WarGalley,
        SceneUnitKind::FireShip,
        SceneUnitKind::DemolitionShip,
        SceneUnitKind::TransportShip,
        SceneUnitKind::VillagerWalk,
        SceneUnitKind::ChopperWork,
        SceneUnitKind::CarryWood,
    ] {
        let slot = kind.actor_slot().unwrap();
        art.actors[slot] = sequence(&art, 16 * 60);
        for row in 0..16 {
            for phase in [0, 31, 59] {
                let u = unit(kind).with_actor_frame(row | phase << 4);
                assert_eq!(frames(&art, u).unwrap().1, 60);
                let draw = world_sprite_frames(&art, &[], &[], &[u], camera(), 0);
                assert_eq!(
                    draw.last().unwrap().1.atlas,
                    art.actors[slot][usize::from(row) * 60 + usize::from(phase)].atlas
                );
                assert!(
                    draw.last().unwrap().0.uv[2] > 0.0,
                    "native directions are never mirrored"
                );
            }
        }
    }
}
#[wasm_bindgen_test]
fn native_fishing_poses_keep_sail_and_fisher_animation_rows() {
    let mut art = art();
    art.actors = vec![Vec::new(); 167];
    for (kind, stride) in [
        (SceneUnitKind::FishingShipWorking, 30),
        (SceneUnitKind::VillagerFishing, 60),
    ] {
        let slot = kind.actor_slot().unwrap();
        art.actors[slot] = sequence(&art, 16 * stride);
        let u = unit(kind).with_actor_frame(15 | ((stride - 1) as u16) << 4);
        assert_eq!(frames(&art, u).unwrap().1, stride);
        let draw = world_sprite_frames(&art, &[], &[], &[u], camera(), 0);
        assert_eq!(
            draw.last().unwrap().1.atlas,
            art.actors[slot][16 * stride - 1].atlas
        );
    }
}
#[wasm_bindgen_test]
fn worker_picking_uses_the_unmirrored_native_hotspot_for_all_sixteen_headings() {
    let mut art = art();
    art.actors = vec![Vec::new(); 167];
    let kind = SceneUnitKind::HunterStand;
    let mut frame = art.village[1][0];
    frame.size = [12.0, 18.0];
    frame.anchor = [2.0, 16.0];
    art.actors[kind.actor_slot().unwrap()] = vec![frame; 16];
    let mut camera = camera();
    camera.center = [0.0; 2];
    for row in 0..16 {
        let mut u = unit(kind).with_actor_frame(row);
        u.position = [0.0; 2];
        let bounds = super::super::scene_unit_bounds(&art, u, camera, 0).unwrap();
        assert!((bounds[0] - (camera.viewport[0] / 2.0 - 2.0 * 4.0 / 3.0)).abs() < 0.001);
    }
}
#[wasm_bindgen_test]
fn work_and_carry_cells_use_their_own_pose_and_missing_art_draws_a_marker() {
    let mut art = art();
    art.actors = vec![Vec::new(); 167];
    for (kind, phase) in [
        (SceneUnitKind::ChopperWork, 7),
        (SceneUnitKind::CarryWood, 0),
    ] {
        let slot = kind.actor_slot().unwrap();
        art.actors[slot] = sequence(&art, 16 * 15);
        let u = unit(kind).with_actor_frame(phase << 4);
        let draw = world_sprite_frames(&art, &[], &[], &[u], camera(), 7);
        assert_eq!(
            draw.last().unwrap().1.atlas,
            art.actors[slot][usize::from(phase)].atlas
        );
    }
    art.actors.clear();
    let draw = world_sprite_frames(
        &art,
        &[],
        &[],
        &[unit(SceneUnitKind::Villager)],
        camera(),
        7,
    );
    assert_eq!(draw.len(), 1);
    assert_eq!(draw[0].0.color, [0.25, 0.57, 0.96, 1.0]);
}

#[wasm_bindgen_test]
fn single_direction_ship_deaths_use_their_dat_row_for_every_native_heading() {
    let mut art = art();
    art.actors = vec![Vec::new(); 167];
    art.actor_rows = vec![16; 167];
    let kind = SceneUnitKind::GalleyDie;
    let slot = kind.actor_slot().unwrap();
    art.actor_rows[slot] = 1;
    art.actors[slot] = sequence(&art, 10);
    for heading in 0..16 {
        let u = unit(kind).with_actor_frame(heading | 9 << 4);
        let draw = world_sprite_frames(&art, &[], &[], &[u], camera(), 0);
        assert_eq!(draw.last().unwrap().1.atlas, art.actors[slot][9].atlas);
        assert!(super::super::scene_unit_bounds(&art, u, camera(), 0).is_some());
    }
}

#[wasm_bindgen_test]
fn rebased_stone_barriers_keep_dat_identity_and_do_not_alias_native_actors() {
    assert_eq!(std::mem::size_of::<SceneUnitKind>(), 1);
    assert_eq!(std::mem::size_of::<SceneUnit>(), 32);
    for (slot, id) in [(71, 117), (72, 64), (73, 64)] {
        let kind = SceneUnitKind::building(slot);
        assert_eq!(kind.building_id(), Some(id));
        assert_eq!(kind.building_slot(), Some(slot));
        assert_eq!(kind.actor_slot(), None);
    }
    for slot in 0..167 {
        assert!(SceneUnitKind::actor(slot).actor_slot().is_some());
        assert_eq!(SceneUnitKind::actor(slot).building_slot(), None);
    }
}

#[wasm_bindgen_test]
fn rebased_stone_gate_uses_the_dat_open_state() {
    let mut art = art();
    let building = &mut art.compositions[0];
    building.unit_id = 64;
    let closed = building.parts[0].clone();
    let mut open = closed.clone();
    open.frame.atlas.uv[0] += 0.25;
    let expected = open.frame.atlas;
    building.parts = vec![closed];
    building.states = vec![(5, vec![open])];
    let draw = world_sprite_frames(
        &art,
        &[],
        &[],
        &[unit(SceneUnitKind::OpenStoneGate)],
        camera(),
        0,
    );
    assert_eq!(draw.last().unwrap().1.atlas, expected);
}

#[wasm_bindgen_test]
fn current_building_slots_round_trip_without_claiming_actor_tags() {
    for slot in 0..=73 {
        let kind = SceneUnitKind::building(slot);
        if slot == 22 {
            assert!(kind.actor_slot().is_some());
            assert_eq!(kind.building_slot(), None);
        } else {
            assert_eq!(kind.building_slot(), Some(slot));
            assert_eq!(kind.actor_slot(), None);
        }
    }
}
