//! Fish Traps (DAT FTRAP 199): built and worked by Fishing Ships on open water.
use super::*;

/// A dock on the coast, a Fishing Ship beside it and a free two by two stretch of water.
fn trap_world() -> (GameWorld, EntityId, EntityId, TileCoord) {
    let mut world = coast();
    world
        .place_town_center(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    let origin = site(&world);
    let dock = build_dock(&mut world, origin);
    let spot = (origin.x + 4..origin.x + 14)
        .flat_map(|x| (origin.y - 3..origin.y + 6).map(move |y| TileCoord::new(x, y)))
        .find(|&tile| {
            world
                .validate_building_placement(PlayerId(0), BuildingKind::FishTrap, tile)
                .is_ok()
        })
        .expect("open water beside the dock");
    let ship = world
        .spawn_kind(
            PlayerId(0),
            UnitKind::FishingShip,
            WorldPosition::from_tile_center(TileCoord::new(spot.x - 1, spot.y)).unwrap(),
        )
        .unwrap();
    fixture::survey(&mut world, PlayerId(0), spot, 8);
    (world, dock, ship, spot)
}

#[test]
fn only_fishing_ships_build_fish_traps_and_only_on_open_water() {
    let (mut world, _, ship, spot) = trap_world();
    let villager = world
        .units()
        .find(|u| u.kind == UnitKind::Villager)
        .unwrap()
        .id;
    assert_eq!(
        world.issue_build(
            PlayerId(0),
            &[villager],
            BuildingKind::FishTrap,
            spot,
            false
        ),
        Err(EconomyError::WrongUnitKind)
    );
    assert_eq!(
        world.issue_build(PlayerId(0), &[ship], BuildingKind::House, spot, false),
        Err(EconomyError::WrongUnitKind)
    );
    // A footprint touching land is refused.
    let land = site(&world);
    assert_eq!(
        world.issue_build(PlayerId(0), &[ship], BuildingKind::FishTrap, land, false),
        Err(EconomyError::InvalidFootprint)
    );
    let wood = world.player_economy(PlayerId(0)).unwrap().stockpile.wood;
    let trap = world
        .issue_build(PlayerId(0), &[ship], BuildingKind::FishTrap, spot, false)
        .unwrap();
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().stockpile.wood,
        wood - 100
    );
    assert!(world.building(trap).unwrap().is_unstarted());
    assert_eq!(
        world.validate_building_placement(PlayerId(0), BuildingKind::FishTrap, spot),
        Err(EconomyError::InvalidFootprint)
    );
    assert_eq!(
        world.validate_building_placement(PlayerId(1), BuildingKind::FishTrap, spot),
        Ok(())
    );
    let rules = BuildingKind::FishTrap.rules();
    advance(&mut world, (rules.build_seconds + 20) * 20);
    let built = world.building(trap).unwrap();
    assert!(built.is_complete());
    assert_eq!((built.max_hp, built.food_remaining), (250, Some(715)));
    assert_eq!(
        world.validate_building_placement(PlayerId(1), BuildingKind::FishTrap, spot),
        Err(EconomyError::InvalidFootprint)
    );
}

#[test]
fn a_fishing_ship_harvests_a_trap_to_the_dock_and_the_spent_trap_collapses() {
    let (mut world, _, ship, spot) = trap_world();
    let trap = world
        .issue_build(PlayerId(0), &[ship], BuildingKind::FishTrap, spot, false)
        .unwrap();
    advance(&mut world, 70 * 20);
    assert!(world.building(trap).unwrap().is_complete());
    world.building_mut(trap).unwrap().food_remaining = Some(30);
    let food = world.player_economy(PlayerId(0)).unwrap().stockpile.food;
    world.issue_farm(PlayerId(0), ship, trap).unwrap();
    // Villagers cannot work it, and a ship cannot work a farm.
    let villager = world
        .units()
        .find(|u| u.kind == UnitKind::Villager)
        .unwrap()
        .id;
    assert_eq!(
        world.issue_farm(PlayerId(0), villager, trap),
        Err(EconomyError::WrongUnitKind)
    );
    advance(&mut world, 200 * 20);
    let gathered = world.player_economy(PlayerId(0)).unwrap().stockpile.food - food;
    assert!(gathered >= 15, "a full load reached the Dock: {gathered}");
    assert!(gathered <= 30);
    // 30 food at 0.35 a second, with two 15-food loads, empties the trap.
    advance(&mut world, 200 * 20);
    assert!(world.building(trap).is_none(), "a spent trap collapses");
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().stockpile.food - food,
        30
    );
}

#[test]
fn fishing_lines_speed_up_trap_fishing() {
    let (mut world, _, ship, spot) = trap_world();
    let trap = world
        .issue_build(PlayerId(0), &[ship], BuildingKind::FishTrap, spot, false)
        .unwrap();
    advance(&mut world, 70 * 20);
    let player = PlayerId(0);
    world.economy.players.get_mut(&player).unwrap().researched |= crate::Tech::FishingLines.bit();
    world.issue_farm(player, ship, trap).unwrap();
    advance(&mut world, 20 * 20);
    let left = world.building(trap).unwrap().food_remaining.unwrap();
    // 20 s at 0.385 food/s is about 7 food, against 6 at the base 0.35.
    assert!((715 - left) >= 7, "caught {}", 715 - left);
}
