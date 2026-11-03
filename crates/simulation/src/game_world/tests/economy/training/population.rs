use super::*;

#[test]
fn initial_population_and_later_spawns_are_not_truncated_to_u16() {
    let mut world = world();
    for index in 0..65_536 {
        let tile = TileCoord::new(100 + index % 128, 100 + (index / 128) % 128);
        world
            .spawn_villager(PlayerId(0), WorldPosition::from_tile_center(tile).unwrap())
            .unwrap();
    }
    world
        .place_town_center(PlayerId(0), TileCoord::new(20, 20))
        .unwrap();
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        65_536
    );
    world
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(230, 230)).unwrap(),
        )
        .unwrap();
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        65_537
    );
    assert_eq!(world.unit_count(), 65_537);
}

#[test]
fn housing_blocks_training_and_removal_preserves_living_units() {
    let (mut world, _, center) = village();
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    advance(&mut world, 500);
    assert_eq!(world.unit_count(), 5);
    assert_eq!(
        world.issue_train(PlayerId(0), center, UnitKind::Villager),
        Err(EconomyError::PopulationCap)
    );
    let house = world
        .insert_building(
            PlayerId(0),
            BuildingKind::House,
            TileRect::new(TileCoord::new(20, 20), TileCoord::new(22, 22)),
            true,
        )
        .unwrap();
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_cap,
        10
    );
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    advance(&mut world, 500);
    assert_eq!(world.unit_count(), 6);
    world.remove_building(house);
    assert_eq!(world.player_economy(PlayerId(0)).unwrap().population_cap, 5);
    assert_eq!(world.unit_count(), 6);
    assert_eq!(
        world.issue_train(PlayerId(0), center, UnitKind::Villager),
        Err(EconomyError::PopulationCap)
    );
}

#[test]
fn training_continues_beyond_the_former_five_thousand_match_ceiling() {
    let (mut world, _, center) = village();
    for index in 0..1_201 {
        let tile = TileCoord::new(2 * (index % 100), 2 * (index / 100));
        world
            .insert_building(
                PlayerId(0),
                BuildingKind::House,
                TileRect::new(tile, TileCoord::new(tile.x + 2, tile.y + 2)),
                true,
            )
            .unwrap();
    }
    for index in 0..6_000 {
        let tile = TileCoord::new(100 + 2 * (index % 75), 60 + 2 * (index / 75));
        world
            .spawn_villager(PlayerId(0), WorldPosition::from_tile_center(tile).unwrap())
            .unwrap();
    }
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_cap,
        6_010
    );
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
    world.advance();
    assert_eq!(world.unit_count(), 6_005);
    assert!(world.building(center).unwrap().training_queue.is_empty());
}
