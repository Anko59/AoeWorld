use super::*;

#[test]
fn training_charges_food_on_acceptance_completes_at_twenty_five_seconds() {
    let (mut world, _, center) = village();
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().stockpile.food,
        150
    );
    assert_eq!(world.building(center).unwrap().training_queue.len(), 1);
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        4
    );
    advance(&mut world, 499);
    assert_eq!(world.unit_count(), 4);
    assert_eq!(
        world.building(center).unwrap().training_queue[0].progress_ticks,
        499
    );
    world.advance();
    assert_eq!(world.unit_count(), 5);
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        5
    );
    assert!(world.building(center).unwrap().training_queue.is_empty());
    let new_unit = world.units().max_by_key(|unit| unit.id).unwrap();
    assert_eq!(new_unit.kind, UnitKind::Villager);
    assert!(world.tile_passable(new_unit.position.tile_floor()));
    assert_ne!(new_unit.id, center);
    assert!(
        world
            .units()
            .filter(|unit| unit.position == new_unit.position)
            .count()
            == 1
    );
}

#[test]
fn queue_reserves_population_and_refusals_are_atomic_and_typed() {
    let (mut world, _, center) = village();
    let before = world.canonical_hash();
    for (player, building, kind, expected) in [
        (
            PlayerId(1),
            center,
            UnitKind::Villager,
            EconomyError::NotOwner,
        ),
        (
            PlayerId(0),
            EntityId(u32::MAX),
            UnitKind::Villager,
            EconomyError::UnknownEntity,
        ),
        (
            PlayerId(0),
            center,
            UnitKind::Scout,
            EconomyError::WrongUnitKind,
        ),
    ] {
        assert_eq!(world.issue_train(player, building, kind), Err(expected));
        assert_eq!(world.canonical_hash(), before);
    }
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    let accepted = world.canonical_hash();
    assert_eq!(
        world.issue_train(PlayerId(0), center, UnitKind::Villager),
        Err(EconomyError::PopulationCap)
    );
    assert_eq!(world.canonical_hash(), accepted);
    advance(&mut world, 500);
    assert_eq!(
        world.issue_train(PlayerId(0), center, UnitKind::Villager),
        Err(EconomyError::PopulationCap)
    );
}

#[test]
fn food_queue_and_serial_training_limits_use_one_rules_table() {
    let (mut world, _, center) = village();
    let player = world.economy.players.get_mut(&PlayerId(0)).unwrap();
    player.population_cap = 20;
    player.stockpile.food = 300;
    for _ in 0..5 {
        world
            .issue_train(PlayerId(0), center, UnitKind::Villager)
            .unwrap();
    }
    let before = world.canonical_hash();
    assert_eq!(
        world.issue_train(PlayerId(0), center, UnitKind::Villager),
        Err(EconomyError::TrainingQueueFull)
    );
    assert_eq!(world.canonical_hash(), before);
    advance(&mut world, 500);
    assert_eq!(world.unit_count(), 5);
    assert_eq!(world.building(center).unwrap().training_queue.len(), 4);
    assert_eq!(
        world.building(center).unwrap().training_queue[0].progress_ticks,
        0
    );
    advance(&mut world, 2000);
    assert_eq!(world.unit_count(), 9);
    assert!(world.building(center).unwrap().training_queue.is_empty());
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().stockpile.food,
        50
    );
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .stockpile
        .food = 49;
    let before = world.canonical_hash();
    assert_eq!(
        world.issue_train(PlayerId(0), center, UnitKind::Villager),
        Err(EconomyError::InsufficientFood)
    );
    assert_eq!(world.canonical_hash(), before);
}

#[test]
fn blocked_spawn_waits_without_losing_the_completed_villager_or_food() {
    let (mut world, _, center) = village();
    let footprint = world.building(center).unwrap().footprint;
    for y in footprint.min.y - 1..=footprint.max.y {
        for x in footprint.min.x - 1..=footprint.max.x {
            let tile = TileCoord::new(x, y);
            if !footprint.contains(tile) && !world.units().any(|u| u.position.tile_floor() == tile)
            {
                world
                    .spawn_unit(PlayerId(1), WorldPosition::from_tile_center(tile).unwrap())
                    .unwrap();
            }
        }
    }
    let count = world.unit_count();
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    advance(&mut world, 600);
    assert_eq!(world.unit_count(), count);
    assert_eq!(
        world.building(center).unwrap().training_queue[0].progress_ticks,
        500
    );
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().stockpile.food,
        150
    );
}

#[test]
fn rally_trained_unit_walks_to_ground_and_changes_the_replay_state() {
    let (mut world, _, center) = village();
    let target = WorldPosition::from_tile_center(TileCoord::new(72, 68)).unwrap();
    let before = world.canonical_hash();
    world
        .issue_set_rally(PlayerId(0), center, crate::RallyTarget::Ground(target))
        .unwrap();
    assert_ne!(before, world.canonical_hash());
    let accepted = world.canonical_hash();
    assert_eq!(
        world.issue_set_rally(PlayerId(1), center, crate::RallyTarget::Ground(target)),
        Err(EconomyError::NotOwner)
    );
    assert_eq!(accepted, world.canonical_hash());
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    advance(&mut world, 1000);
    let trained = world.units().max_by_key(|u| u.id).unwrap();
    assert_eq!(trained.position, target);
    assert_eq!(
        world
            .building(center)
            .unwrap()
            .rally_point
            .unwrap()
            .position,
        target
    );
}

#[test]
fn rally_trained_villager_gathers_the_resource() {
    let (package, pages, water) = fixture::flat_fixture(false);
    let mut world =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    world
        .initialize_village(PlayerId(0), TileCoord::new(64, 64))
        .unwrap();
    let center = world.buildings().next().unwrap().id;
    let node = world
        .starting_resources()
        .into_iter()
        .find(|n| n.kind == ResourceKind::Food)
        .unwrap();
    world.set_fog_disabled(true);
    world
        .issue_set_rally(PlayerId(0), center, crate::RallyTarget::Resource(node.id))
        .unwrap();
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    advance(&mut world, 1000);
    let trained = world.units().max_by_key(|u| u.id).unwrap();
    assert_eq!(world.gather_target(trained.id), Some(node.id));
    assert!(world.resource_remaining(node.id).unwrap() < node.initial_amount);
}

#[test]
fn rally_to_a_friendly_foundation_sends_the_trained_villager_to_build() {
    let (mut world, _, center) = village();
    let builder = world
        .units()
        .find(|u| u.kind == UnitKind::Villager)
        .unwrap()
        .id;
    let house = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            TileCoord::new(75, 70),
        )
        .unwrap();
    world.issue_stop(PlayerId(0), builder).unwrap();
    world
        .issue_set_rally(PlayerId(0), center, crate::RallyTarget::Building(house))
        .unwrap();
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    advance(&mut world, 2000);
    assert!(world.building(house).unwrap().is_complete());
}

#[test]
fn rally_rejects_invalid_targets_atomically() {
    let (mut world, _, center) = village();
    let before = world.canonical_hash();
    for target in [
        crate::RallyTarget::Ground(WorldPosition::new(-1, 0)),
        crate::RallyTarget::Resource(u64::MAX),
        crate::RallyTarget::Building(EntityId(u32::MAX)),
        crate::RallyTarget::Animal(EntityId(u32::MAX)),
    ] {
        assert!(world.issue_set_rally(PlayerId(0), center, target).is_err());
        assert_eq!(world.canonical_hash(), before);
    }
}

#[test]
fn rally_accepts_a_remembered_resource_but_rejects_an_unexplored_one() {
    let (mut world, node, worker) = resource_world(ResourceKind::Food);
    world
        .initialize_village(PlayerId(0), TileCoord::new(128, 128))
        .unwrap();
    let center = world.buildings().next().unwrap().id;
    world.remove_unit(worker);
    assert_eq!(
        world.visibility(PlayerId(0), node.tile),
        crate::TileVisibility::Explored
    );
    world
        .issue_set_rally(PlayerId(0), center, crate::RallyTarget::Resource(node.id))
        .unwrap();
    assert_eq!(
        world.building(center).unwrap().rally_point.unwrap().target,
        crate::RallyTarget::Resource(node.id)
    );
    // Another seat has never seen the node, even though it is a valid resource.
    let other = world
        .initialize_village(PlayerId(1), TileCoord::new(200, 200))
        .unwrap();
    assert!(world.unit(other).is_some());
    let other_center = world
        .buildings()
        .find(|b| b.player == PlayerId(1))
        .unwrap()
        .id;
    assert_eq!(
        world.visibility(PlayerId(1), node.tile),
        crate::TileVisibility::Unexplored
    );
    assert_eq!(
        world.issue_set_rally(
            PlayerId(1),
            other_center,
            crate::RallyTarget::Resource(node.id)
        ),
        Err(EconomyError::InvalidResource)
    );
}
