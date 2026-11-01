//! A 100-seat match at the shipped 1000 population with Feudal-sized villages:
//! the world capacities hold, and placing a building still works.
//!
//! The owner's report: "The order queue is full" on every build order soon
//! after Feudal Age in a 100-player France match. Two world-wide limits were
//! reached: 4,096 buildings in all, and 4,096 building collision tiles in the
//! map overlay (100 Town Centers alone take 1,600). Both refused the order as
//! `Capacity`, which the server reported as a full order queue.
use super::*;
use crate::game_world::Stockpile;

const SEATS: u16 = 100;
const WIDTH: i32 = 4_096;
/// Barren ground: no generated trees or other blocking resources.
const BARREN: u8 = 28;

fn match_world() -> GameWorld {
    let config = WorldConfig::new(WIDTH, WIDTH, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    world.terrain = super::super::far_orders::prepared_terrain(vec![0; 4], vec![BARREN; 4]);
    world.set_limits(WorldLimits::for_match(usize::from(SEATS)));
    world
}

fn rect(x: i32, y: i32, kind: BuildingKind) -> TileRect {
    let rules = kind.rules();
    TileRect::new(
        TileCoord::new(x, y),
        TileCoord::new(x + rules.side, y + rules.depth),
    )
}

/// A Feudal village in its 400-tile cell: a Town Center, 40 houses, the
/// economy and military buildings, 20 farms and a 64-segment palisade.
fn settle(world: &mut GameWorld, player: PlayerId, ox: i32, oy: i32) -> usize {
    let mut placed = Vec::new();
    placed.push((BuildingKind::TownCenter, ox + 10, oy + 10));
    for i in 0..40 {
        placed.push((
            BuildingKind::House,
            ox + 20 + 3 * (i % 10),
            oy + 20 + 3 * (i / 10),
        ));
    }
    let singles = [
        BuildingKind::LumberCamp,
        BuildingKind::MiningCamp,
        BuildingKind::Mill,
        BuildingKind::Barracks,
        BuildingKind::ArcheryRange,
        BuildingKind::Stable,
        BuildingKind::Blacksmith,
        BuildingKind::Market,
        BuildingKind::Outpost,
        BuildingKind::WatchTower,
    ];
    for (i, kind) in singles.into_iter().enumerate() {
        placed.push((kind, ox + 20 + 5 * i as i32, oy + 40));
    }
    for i in 0..20 {
        placed.push((
            BuildingKind::Farm,
            ox + 20 + 4 * (i % 10),
            oy + 50 + 4 * (i / 10),
        ));
    }
    for i in 0..64 {
        placed.push((BuildingKind::PalisadeWall, ox + 10 + i, oy + 70));
    }
    for (kind, x, y) in &placed {
        world
            .insert_building(player, *kind, rect(*x, *y, *kind), true)
            .unwrap_or_else(|error| panic!("{kind:?} of {player:?}: {error:?}"));
    }
    placed.len()
}

#[test]
fn a_hundred_feudal_villages_fit_and_building_orders_still_succeed() {
    let mut world = match_world();
    let cell = WIDTH / 10;
    let mut buildings = 0;
    let mut villagers = Vec::new();
    for seat in 0..SEATS {
        let player = PlayerId(seat);
        let (ox, oy) = (i32::from(seat % 10) * cell, i32::from(seat / 10) * cell);
        buildings += settle(&mut world, player, ox, oy);
        world.economy.players.get_mut(&player).unwrap().stockpile = Stockpile {
            food: 10_000,
            wood: 10_000,
            gold: 10_000,
            stone: 10_000,
        };
        let at = WorldPosition::from_tile_center(TileCoord::new(ox + 100, oy + 100)).unwrap();
        villagers.push((player, world.spawn_villager(player, at).unwrap(), ox, oy));
    }
    assert_eq!(buildings, 135 * usize::from(SEATS));
    assert_eq!(world.buildings().count(), buildings);
    // The old world-wide limits: 4,096 buildings and 4,096 collision tiles.
    assert!(world.buildings().count() > 4_096);
    assert!(world.economy.blocked_tiles.len() > 4_096);
    for (player, villager, ox, oy) in &villagers {
        let origin = TileCoord::new(ox + 110, oy + 100);
        super::super::economy::fixture::survey(&mut world, *player, origin, 2);
        let house = world
            .issue_build(*player, &[*villager], BuildingKind::House, origin, false)
            .unwrap_or_else(|error| panic!("house of {player:?}: {error:?}"));
        assert_eq!(world.building(house).unwrap().player, *player);
    }
    let started = std::time::Instant::now();
    for _ in 0..20 {
        world.advance();
    }
    eprintln!(
        "{} buildings, {} collision tiles: 20 ticks in {:?}",
        world.buildings().count(),
        world.economy.blocked_tiles.len(),
        started.elapsed()
    );
}

#[test]
fn a_full_world_refuses_as_a_world_limit() {
    let mut world = match_world();
    settle(&mut world, PlayerId(0), 0, 0);
    let player = PlayerId(0);
    world
        .economy
        .players
        .get_mut(&player)
        .unwrap()
        .stockpile
        .wood = 10_000;
    let at = WorldPosition::from_tile_center(TileCoord::new(100, 100)).unwrap();
    let villager = world.spawn_villager(player, at).unwrap();
    // The play/integration limits: 4,096 buildings for the whole world.
    world.set_limits(WorldLimits {
        buildings: world.buildings().count(),
        ..world.limits()
    });
    assert_eq!(
        world.issue_build(
            player,
            &[villager],
            BuildingKind::House,
            TileCoord::new(110, 100),
            false
        ),
        Err(EconomyError::Capacity)
    );
    assert_eq!(
        world.issue_build(
            player,
            &[],
            BuildingKind::House,
            TileCoord::new(110, 100),
            false
        ),
        Err(EconomyError::OrderTooLarge)
    );
}

#[test]
fn match_limits_provide_bounded_storage_for_every_seat() {
    let limits = WorldLimits::for_match(100);
    assert_eq!(limits.buildings, 100 * 8_192);
    assert_eq!(limits.gatherers, 100 * 65_536);
    assert_eq!(limits.builders, 100 * 65_536);
    assert_eq!(limits.hunters, 100 * 65_536);
    assert_eq!(limits.animals, 100 * crate::ANIMALS_PER_PLAYER);
    // The overlay's collision ceiling holds every building of the largest world.
    let largest = WorldLimits::for_match(crate::game_world::economy::MAX_ECONOMY_PLAYERS);
    assert!(largest.buildings as u64 * 16 <= aoe_map::MAX_OBSTACLE_TILES);
}
