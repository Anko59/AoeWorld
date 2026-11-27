use super::*;

#[test]
fn one_wall_order_accepts_more_than_a_thousand_segments() {
    let config = aoe_core::WorldConfig::new(1_100, 32, aoe_core::Seed(7)).unwrap();
    let mut world = GameWorld::new(config).unwrap();
    world.set_fog_disabled(true);
    let player = PlayerId(0);
    world.place_town_center(player, at(16, 16)).unwrap();
    set_stock(&mut world, 0, 10_000, 0, 0);
    let from = at(5, 24);
    let to = at(1_074, 24);
    let before = world.canonical_hash();
    assert_eq!(
        world.issue_build_wall(player, &[], BuildingKind::PalisadeWall, from, to, false),
        Err(EconomyError::Movement(crate::GameWorldError::Unreachable)),
    );
    assert_eq!(world.canonical_hash(), before);

    villager_at(&mut world, 5, 23);
    let ids = world
        .issue_build_wall(player, &[], BuildingKind::PalisadeWall, from, to, false)
        .unwrap();
    assert_eq!(ids.len(), 1_070);
    assert_eq!(world.building(ids[0]).unwrap().footprint.min, from);
    assert_eq!(
        world.building(*ids.last().unwrap()).unwrap().footprint.min,
        to
    );
    assert_eq!(stock(&world).1, 10_000 - 2 * 1_070);

    let before = world.canonical_hash();
    let beyond_map = at(1_104, 24);
    assert_eq!(
        world.issue_build_wall(
            player,
            &[],
            BuildingKind::PalisadeWall,
            from,
            beyond_map,
            false,
        ),
        Err(EconomyError::InvalidFootprint),
    );
    assert_eq!(world.canonical_hash(), before);
}

#[test]
fn a_wall_order_skips_its_occupied_nearest_segment() {
    let (mut world, _, _) = village();
    world.set_fog_disabled(true);
    let player = PlayerId(0);
    let builder = villager_at(&mut world, 102, 99);
    world
        .spawn_villager(
            PlayerId(1),
            WorldPosition::from_tile_center(at(102, 100)).unwrap(),
        )
        .unwrap();
    let ids = world
        .issue_build_wall(
            player,
            &[builder],
            BuildingKind::PalisadeWall,
            at(100, 100),
            at(104, 100),
            false,
        )
        .unwrap();
    assert_eq!(ids.len(), 5);
    assert_eq!(world.economy.builders[&builder].building, ids[1]);
    assert!(
        world
            .economy
            .builders
            .values()
            .all(|job| job.building != ids[2])
    );
    assert!(world.economy.dispatch.sites.contains_key(&ids[2]));
}

#[test]
fn a_single_build_order_accepts_one_hundred_villagers() {
    let (mut world, _, _) = village();
    world.set_fog_disabled(true);
    let builders = (0..100)
        .map(|n| villager_at(&mut world, 90 + n % 10, 90 + n / 10))
        .collect::<Vec<_>>();
    let foundation = world
        .issue_build(
            PlayerId(0),
            &builders,
            BuildingKind::House,
            at(101, 101),
            false,
        )
        .unwrap();
    assert_eq!(
        world
            .economy
            .builders
            .values()
            .filter(|job| job.building == foundation)
            .count(),
        100,
    );
}
