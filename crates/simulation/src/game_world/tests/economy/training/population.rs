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
fn training_finishes_without_houses_or_a_population_cap() {
    let (mut world, _, center) = village();
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .stockpile
        .food = 400;
    for _ in 0..6 {
        world
            .issue_train(PlayerId(0), center, UnitKind::Villager)
            .unwrap();
    }
    advance(&mut world, 3_000);
    assert!(world.building(center).unwrap().training_queue.is_empty());
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        10
    );
    assert_eq!(world.unit_count(), 10);
    assert!(world.buildings().all(|b| b.kind != BuildingKind::House));
}

#[test]
fn training_continues_beyond_the_former_five_thousand_match_ceiling() {
    let (mut world, _, center) = village();
    for index in 0..6_000 {
        let tile = TileCoord::new(100 + 2 * (index % 75), 2 * (index / 75));
        world
            .spawn_villager(PlayerId(0), WorldPosition::from_tile_center(tile).unwrap())
            .unwrap();
    }
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        6_004
    );
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    // Duration is covered separately. Exercise the actual completion tick at
    // this population without advancing thousands of idle actors for 25s.
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
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        6_005
    );
    assert!(world.building(center).unwrap().training_queue.is_empty());
}
