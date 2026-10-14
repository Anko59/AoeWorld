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
