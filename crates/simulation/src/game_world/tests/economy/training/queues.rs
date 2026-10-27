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
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .population_cap = 20;
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
fn fifteen_unit_limit_is_per_building_and_cancellation_refunds_a_housed_wait() {
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
    player.population_cap = 4;
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
    advance(&mut world, 600);
    for center in [first, second] {
        let queue = &world.building(center).unwrap().training_queue;
        assert_eq!(queue.len(), 15);
        assert_eq!(queue[0].progress_ticks, 0);
    }
    world.issue_cancel_training(PlayerId(0), second).unwrap();
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().stockpile.food,
        50
    );
    assert_eq!(world.building(second).unwrap().training_queue.len(), 14);
    // Bundled behavior: both full queues resume when one house slot opens,
    // but only one may spawn, at its front. The other retains its partial work.
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .population_cap = 5;
    advance(&mut world, 499);
    for center in [first, second] {
        assert_eq!(
            world.building(center).unwrap().training_queue[0].progress_ticks,
            499
        );
    }
    world.advance();
    let spawned = world.units().max_by_key(|u| u.id).unwrap();
    let spawned_id = spawned.id;
    assert_eq!(
        spawned.position.tile_floor(),
        world.building(first).unwrap().footprint.max
    );
    assert_eq!(world.building(first).unwrap().training_queue.len(), 14);
    assert_eq!(
        world.building(second).unwrap().training_queue[0].progress_ticks,
        499
    );
    advance(&mut world, 600);
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        5
    );
    assert_eq!(
        world.building(first).unwrap().training_queue[0].progress_ticks,
        0
    );
    // Death frees population immediately; removing an index entry alone does not.
    world.kill_unit(world.lookup[&spawned_id]);
    world.advance();
    assert_eq!(world.building(second).unwrap().training_queue.len(), 13);
    assert_eq!(
        world
            .units()
            .max_by_key(|u| u.id)
            .unwrap()
            .position
            .tile_floor(),
        world.building(second).unwrap().footprint.max
    );
}
