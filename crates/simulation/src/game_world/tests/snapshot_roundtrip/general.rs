//! General ownership and paid recruitment survive a save.
use super::*;

#[test]
fn restored_scout_skips_a_blocked_current_leg_and_finishes_its_queued_destination() {
    let (mut saved, start) = flat();
    let at = |dx, dy| {
        aoe_core::WorldPosition::from_tile_center(TileCoord::new(start.x + dx, start.y + dy))
            .unwrap()
    };
    let scout = saved
        .spawn_kind(PlayerId(0), crate::UnitKind::Scout, at(6, 7))
        .unwrap();
    let blocked = at(12, 7);
    let later = at(18, 7);
    saved.issue_move(scout, blocked).unwrap();
    saved.issue_queued_move(scout, later).unwrap();
    saved
        .insert_building(
            PlayerId(0),
            crate::BuildingKind::House,
            aoe_core::TileRect::from_xywh(start.x + 12, start.y + 7, 2, 2),
            false,
        )
        .unwrap();
    assert_eq!(saved.movement_goal(scout), Some(blocked));
    assert_eq!(saved.move_destinations(scout), [blocked, later]);
    let (mut restored, report) = roundtrip(&saved, flat().0);
    assert_eq!(report.units_lost, 0);
    assert_eq!(report.buildings_lost, 0);
    assert_eq!(report.jobs_lost, 1, "the foundation blocks the current leg");
    for _ in 0..1000 {
        restored.advance();
    }
    assert_eq!(
        restored.unit(scout).unwrap().position,
        later,
        "losing the current leg on restore must wake the later valid waypoint"
    );
    assert!(restored.move_destinations(scout).is_empty());
}

#[test]
fn restored_sheltered_general_keeps_his_outdoor_army() {
    let (mut saved, start) = flat();
    saved.initialize_village(PlayerId(0), start).unwrap();
    let castle = saved
        .insert_building(
            PlayerId(0),
            crate::BuildingKind::Castle,
            aoe_core::TileRect::from_xywh(start.x + 10, start.y + 6, 4, 4),
            true,
        )
        .unwrap();
    let soldier = saved
        .spawn_kind(
            PlayerId(0),
            crate::UnitKind::Militia,
            aoe_core::WorldPosition::from_tile_center(TileCoord::new(start.x + 8, start.y + 6))
                .unwrap(),
        )
        .unwrap();
    let general = saved
        .spawn_kind(
            PlayerId(0),
            crate::UnitKind::General,
            aoe_core::WorldPosition::from_tile_center(TileCoord::new(start.x + 9, start.y + 6))
                .unwrap(),
        )
        .unwrap();
    assert_eq!(saved.general_controller(soldier), Some(general));
    saved.issue_garrison(PlayerId(0), general, castle).unwrap();
    for _ in 0..200 {
        saved.advance();
    }
    assert!(
        saved
            .building(castle)
            .unwrap()
            .garrison
            .iter()
            .any(|g| g.unit.id == general)
    );
    assert_eq!(saved.general_controller(soldier), Some(general));
    let snapshot = saved.snapshot([0; 32]).unwrap();
    let (mut restored, _) = flat();
    restored
        .restore_snapshot(&snapshot, [0; 32], &|| false)
        .unwrap();
    assert_eq!(restored.general_controller(soldier), Some(general));
}

#[test]
fn restored_general_keeps_explicit_recruits_and_permanent_detachment() {
    let (mut saved, start) = flat();
    let at = |dx, dy| {
        aoe_core::WorldPosition::from_tile_center(TileCoord::new(start.x + dx, start.y + dy))
            .unwrap()
    };
    let detached = saved
        .spawn_kind(PlayerId(0), crate::UnitKind::Militia, at(6, 7))
        .unwrap();
    let general = saved
        .spawn_kind(PlayerId(0), crate::UnitKind::General, at(6, 6))
        .unwrap();
    saved.general_player_order(PlayerId(0), &[detached], None);
    let recruit = saved
        .spawn_kind(PlayerId(0), crate::UnitKind::Militia, at(7, 7))
        .unwrap();
    // The same completion callback that assigns a soldier bought with Train.
    let trainer = EntityId(9999);
    saved
        .economy
        .generals
        .recruits
        .insert(trainer, (general, crate::UnitKind::Militia));
    saved.finish_general_recruit(trainer, recruit);
    assert_eq!(saved.general_controller(recruit), Some(general));
    let free = saved
        .spawn_kind(PlayerId(0), crate::UnitKind::Militia, at(8, 7))
        .unwrap();
    let snapshot = saved.snapshot([0; 32]).unwrap();
    let bytes = postcard::to_allocvec(&snapshot).unwrap();
    let decoded: WorldSnapshot = postcard::from_bytes(&bytes).unwrap();
    let (mut restored, _) = flat();
    restored
        .restore_snapshot(&decoded, [0; 32], &|| false)
        .unwrap();
    assert_eq!(restored.general_controller(recruit), Some(general));
    assert_eq!(restored.general_controller(detached), None);
    assert_eq!(restored.general_controller(free), None);
    assert_eq!(restored.general_detached_count(), 1);
    restored.general_player_order(PlayerId(0), &[general], None);
    for _ in 0..40 {
        restored.advance();
    }
    assert_eq!(restored.general_controller(recruit), Some(general));
    assert_eq!(restored.general_controller(detached), None);
    assert_eq!(restored.general_controller(free), None);
}

#[test]
fn restored_general_keeps_the_assignment_of_a_recruit_still_in_training() {
    let (mut saved, start) = flat();
    saved.initialize_village(PlayerId(0), start).unwrap();
    let player = saved.economy.players.get_mut(&PlayerId(0)).unwrap();
    player.age = crate::Age::Castle;
    player.stockpile = crate::Stockpile {
        food: 10_000,
        wood: 10_000,
        gold: 10_000,
        stone: 10_000,
    };
    saved
        .insert_building(
            PlayerId(0),
            crate::BuildingKind::House,
            aoe_core::TileRect::from_xywh(start.x + 6, start.y + 10, 2, 2),
            true,
        )
        .unwrap();
    let barracks = saved
        .insert_building(
            PlayerId(0),
            crate::BuildingKind::Barracks,
            aoe_core::TileRect::from_xywh(start.x + 10, start.y + 6, 3, 3),
            true,
        )
        .unwrap();
    let general = saved
        .spawn_kind(
            PlayerId(0),
            crate::UnitKind::General,
            aoe_core::WorldPosition::from_tile_center(TileCoord::new(start.x + 15, start.y + 8))
                .unwrap(),
        )
        .unwrap();
    saved
        .issue_general_mix(PlayerId(0), general, [100, 0, 0, 0, 0])
        .unwrap();
    saved
        .issue_general_training(PlayerId(0), general, true)
        .unwrap();
    for _ in 0..30 {
        saved.advance();
    }
    let reservation = saved
        .economy
        .generals
        .recruits
        .get(&barracks)
        .copied()
        .expect("a paid General recruit is training");
    saved
        .issue_general_training(PlayerId(0), general, false)
        .unwrap();
    let snapshot = saved.snapshot([0; 32]).unwrap();
    let (mut restored, _) = flat();
    restored
        .restore_snapshot(&snapshot, [0; 32], &|| false)
        .unwrap();
    assert_eq!(
        restored.economy.generals.recruits.get(&barracks),
        Some(&reservation)
    );
    for _ in 0..1200 {
        restored.advance();
    }
    let recruit = restored
        .units()
        .find(|u| u64::from(u.id.0) >= snapshot.next_entity_id && u.kind == reservation.1)
        .expect("paid training finishes");
    assert_eq!(restored.general_controller(recruit.id), Some(general));
}
