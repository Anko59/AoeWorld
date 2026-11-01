use super::*;

#[test]
fn idle_production_work_does_not_grow_with_idle_buildings() {
    let (mut world, _, _) = village();
    for player in 1..=50 {
        world
            .place_town_center(
                PlayerId(player),
                TileCoord::new(
                    12 + i32::from(player % 10) * 20,
                    120 + i32::from(player / 10) * 20,
                ),
            )
            .unwrap();
    }
    world.advance_research(&mut Vec::new());
    world.advance();
    assert_eq!(world.economy.tick_work.research_buildings, 0);
    assert_eq!(world.economy.tick_work.training_buildings, 0);
}

#[test]
fn completed_training_checks_local_occupants_and_retries_a_freed_exit() {
    let (mut world, _, center) = village();
    let footprint = world.building(center).unwrap().footprint;
    let mut blockers = Vec::new();
    for tile in crate::game_world::economy::placement::perimeter(footprint) {
        if !world.units().any(|u| u.position.tile_floor() == tile) {
            let id = world
                .spawn_unit(PlayerId(1), WorldPosition::from_tile_center(tile).unwrap())
                .unwrap();
            blockers.push((id, tile));
        }
    }
    // A remote army must not be inspected once per spawn candidate.
    for _ in 0..2_000 {
        world
            .spawn_unit(
                PlayerId(1),
                WorldPosition::from_tile_center(TileCoord::new(220, 220)).unwrap(),
            )
            .unwrap();
    }
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    let entry = world
        .economy
        .buildings
        .get_mut(&center)
        .unwrap()
        .training_queue
        .front_mut()
        .unwrap();
    entry.progress_ticks = entry.total_ticks - 1;
    let count = world.unit_count();
    world.advance();
    assert_eq!(world.unit_count(), count, "all exits are occupied");
    assert!(
        world.economy.tick_work.spawn_units < 512,
        "remote units were scanned: {:?}",
        world.economy.tick_work
    );
    let (blocker, freed) = blockers.pop().unwrap();
    world.remove_unit(blocker);
    let trained = EntityId(world.entity_id_bound() as u32);
    let changed = world.advance();
    assert!(changed.iter().any(|unit| unit.id == trained));
    assert!(
        world.economy.tick_work.spawn_units < 512,
        "free exits scanned the remote army: {:?}",
        world.economy.tick_work
    );
    assert_eq!(world.unit(trained).unwrap().position.tile_floor(), freed);
    assert!(world.building(center).unwrap().training_queue.is_empty());
    world.advance();
    assert_eq!(world.economy.tick_work.training_buildings, 0);
}

#[test]
fn cancelled_and_destroyed_queues_stop_work_without_losing_serial_progress() {
    let (mut world, _, center) = village();
    for _ in 0..2 {
        world
            .issue_train(PlayerId(0), center, UnitKind::Villager)
            .unwrap();
    }
    world.advance();
    world.issue_cancel_training(PlayerId(0), center).unwrap();
    world.advance();
    assert_eq!(world.economy.tick_work.training_buildings, 1);
    assert_eq!(
        world.building(center).unwrap().training_queue[0].progress_ticks,
        1
    );
    world.issue_cancel_training(PlayerId(0), center).unwrap();
    world.advance();
    assert_eq!(world.economy.tick_work.training_buildings, 0);
    world
        .issue_research(PlayerId(0), center, crate::Tech::Loom)
        .unwrap();
    world.advance_research(&mut Vec::new());
    assert_eq!(world.economy.tick_work.research_buildings, 1);
    world.issue_cancel_research(PlayerId(0), center).unwrap();
    world.advance_research(&mut Vec::new());
    assert_eq!(world.economy.tick_work.research_buildings, 0);
    world
        .issue_research(PlayerId(0), center, crate::Tech::Loom)
        .unwrap();
    world.remove_building(center);
    world.advance_research(&mut Vec::new());
    world.advance();
    assert_eq!(world.economy.tick_work.research_buildings, 0);
    assert_eq!(world.economy.tick_work.training_buildings, 0);

    let other = world
        .place_town_center(PlayerId(0), TileCoord::new(180, 180))
        .unwrap();
    world
        .issue_train(PlayerId(0), other, UnitKind::Villager)
        .unwrap();
    world.remove_building(other);
    world.advance();
    assert_eq!(world.economy.tick_work.training_buildings, 0);
}

#[test]
fn fifteen_unit_limit_is_per_building_and_cancellation_refunds_queued_training() {
    let (mut world, _, first) = village();
    let second = world
        .insert_building(
            PlayerId(0),
            BuildingKind::TownCenter,
            aoe_core::TileRect::from_xywh(90, 90, 4, 4),
            true,
        )
        .unwrap();
    let player = world.economy.players.get_mut(&PlayerId(0)).unwrap();
    player.stockpile.food = 1500;
    for center in [first, second] {
        for _ in 0..15 {
            world
                .issue_train(PlayerId(0), center, UnitKind::Villager)
                .unwrap();
        }
        let full = world.canonical_hash();
        assert_eq!(
            world.issue_train(PlayerId(0), center, UnitKind::Villager),
            Err(EconomyError::TrainingQueueFull)
        );
        assert_eq!(world.canonical_hash(), full);
    }
    world.issue_cancel_training(PlayerId(0), second).unwrap();
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().stockpile.food,
        50
    );
    assert_eq!(world.building(second).unwrap().training_queue.len(), 14);
    let previous_ids: std::collections::BTreeSet<_> = world.units().map(|u| u.id).collect();
    advance(&mut world, 499);
    for center in [first, second] {
        assert_eq!(
            world.building(center).unwrap().training_queue[0].progress_ticks,
            499
        );
    }
    world.advance();
    let spawned: Vec<_> = world
        .units()
        .filter(|u| !previous_ids.contains(&u.id))
        .collect();
    assert_eq!(spawned.len(), 2, "both producers finish without housing");
    for center in [first, second] {
        assert!(
            spawned
                .iter()
                .any(|u| u.position.tile_floor() == world.building(center).unwrap().footprint.max)
        );
    }
    let spawned_id = spawned[0].id;
    assert_eq!(world.building(first).unwrap().training_queue.len(), 14);
    assert_eq!(world.building(second).unwrap().training_queue.len(), 13);
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        6
    );
    for center in [first, second] {
        assert_eq!(
            world.building(center).unwrap().training_queue[0].progress_ticks,
            0
        );
    }
    // Death changes the count immediately, and removing its corpse cannot
    // decrement it a second time. Neither event gates the other producer.
    world.kill_unit(world.lookup[&spawned_id]);
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        5
    );
    world.remove_unit(spawned_id);
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        5
    );
    world.advance();
    for center in [first, second] {
        assert_eq!(
            world.building(center).unwrap().training_queue[0].progress_ticks,
            1
        );
    }
}
