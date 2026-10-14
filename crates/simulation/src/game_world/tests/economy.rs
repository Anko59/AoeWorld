use super::*;
use crate::{BuildingKind, CarriedResource, EconomyError, UnitKind, UnitTask, VILLAGE_RULES};
use aoe_map::{ResourceKind, ResourceNode};

mod gathering;
mod training;

fn village() -> (GameWorld, EntityId, EntityId) {
    let mut world = world();
    let scout = world
        .initialize_village(PlayerId(0), TileCoord::new(64, 64))
        .unwrap();
    let center = world.buildings().next().unwrap().id;
    (world, scout, center)
}

fn resource_world(kind: ResourceKind) -> (GameWorld, ResourceNode, EntityId) {
    let package = MapPackage::new(1, MapRequest::default(), Vec::new()).unwrap();
    let generator = package.generator();
    let mut world = GameWorld::from_map(package).unwrap();
    for y in 0..8 {
        for x in 0..8 {
            for node in generator.chunk(x, y).unwrap().resources {
                if node.kind != kind {
                    continue;
                }
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let tile = TileCoord::new(node.tile.x + dx, node.tile.y + dy);
                        if world.terrain().passable(tile, world.config()) {
                            let id = world
                                .spawn_villager(
                                    PlayerId(0),
                                    WorldPosition::from_tile_center(tile).unwrap(),
                                )
                                .unwrap();
                            return (world, node, id);
                        }
                    }
                }
            }
        }
    }
    panic!("fixture needs an accessible resource of {kind:?}");
}

fn advance(world: &mut GameWorld, ticks: u32) {
    for _ in 0..ticks {
        world.advance();
    }
}

#[test]
fn start_has_one_center_three_villagers_scout_and_standard_stockpile() {
    let (world, scout, center) = village();
    assert_eq!(world.unit_count(), 4);
    assert_eq!(world.unit(scout).unwrap().kind, UnitKind::Scout);
    assert_eq!(
        world
            .units()
            .filter(|u| u.kind == UnitKind::Villager)
            .count(),
        3
    );
    let building = world.building(center).unwrap();
    assert_eq!(building.kind, BuildingKind::TownCenter);
    assert_eq!(building.player, PlayerId(0));
    assert_eq!(building.footprint.width(), 4);
    assert_eq!(building.footprint.height(), 4);
    let player = world.player_economy(PlayerId(0)).unwrap();
    assert_eq!(
        (
            player.stockpile.food,
            player.stockpile.wood,
            player.stockpile.gold,
            player.stockpile.stone
        ),
        (200, 200, 100, 200)
    );
    assert_eq!((player.population_used, player.population_cap), (4, 5));
    assert_eq!(VILLAGE_RULES.villager_capacity, 10);
    let mut ids = world.units().map(|u| u.id).collect::<Vec<_>>();
    ids.push(center);
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 5, "buildings share the entity allocator");
    for y in building.footprint.min.y..building.footprint.max.y {
        for x in building.footprint.min.x..building.footprint.max.x {
            assert!(!world.tile_passable(TileCoord::new(x, y)));
        }
    }
    for unit in world.units() {
        assert!(world.tile_passable(unit.position.tile_floor()));
        assert!(
            unit.position
                .tile_floor()
                .x
                .abs_diff(building.footprint.min.x)
                <= 8
        );
    }
}

#[test]
fn footprint_cannot_cover_resources_water_units_or_map_edges() {
    let (mut world, node, _) = resource_world(ResourceKind::Wood);
    assert!(world.place_town_center(PlayerId(0), node.tile).is_err());
    assert_eq!(world.buildings().count(), 0);
    let mut flat = super::world();
    assert!(
        flat.place_town_center(PlayerId(0), TileCoord::new(254, 254))
            .is_err()
    );
    let occupied = TileCoord::new(20, 20);
    flat.spawn_unit(
        PlayerId(0),
        WorldPosition::from_tile_center(occupied).unwrap(),
    )
    .unwrap();
    assert!(flat.place_town_center(PlayerId(0), occupied).is_err());
}

#[test]
fn village_search_is_bounded_and_moves_off_an_invalid_footprint() {
    let (mut world, node, _) = resource_world(ResourceKind::Wood);
    let scout = world.initialize_village(PlayerId(1), node.tile).unwrap();
    assert!(world.unit(scout).is_some());
    let center = world.buildings().next().unwrap();
    assert!(!center.footprint.contains(node.tile));
    assert!(center.footprint.min.x.abs_diff(node.tile.x) <= 32);
}

#[test]
fn movement_routes_around_the_impassable_center_and_rejects_its_interior() {
    let (mut world, scout, center) = village();
    let footprint = world.building(center).unwrap().footprint;
    let inside = WorldPosition::from_tile_center(footprint.min).unwrap();
    assert_eq!(
        world.issue_move(scout, inside),
        Err(GameWorldError::InvalidPosition)
    );
    let target = TileCoord::new(footprint.max.x + 2, footprint.max.y + 2);
    world
        .issue_move(scout, WorldPosition::from_tile_center(target).unwrap())
        .unwrap();
    for _ in 0..1000 {
        world.advance();
        assert!(!footprint.contains(world.unit(scout).unwrap().position.tile_floor()));
    }
    assert_eq!(world.unit(scout).unwrap().position.tile_floor(), target);
}

#[test]
fn villagers_move_at_point_eight_tiles_per_second_and_scout_keeps_its_speed() {
    let mut world = world();
    let start = WorldPosition::from_tile_center(TileCoord::new(20, 20)).unwrap();
    let villager = world.spawn_villager(PlayerId(0), start).unwrap();
    let scout = world.spawn_unit(PlayerId(0), start).unwrap();
    let destination = WorldPosition::from_tile_center(TileCoord::new(60, 20)).unwrap();
    world.issue_move(villager, destination).unwrap();
    world.issue_move(scout, destination).unwrap();
    let scout_speed = world.config().move_speed_subunits_per_tick;
    advance(&mut world, 100);
    assert_eq!(world.unit(villager).unwrap().position.x - start.x, 4096);
    assert_eq!(
        world.unit(scout).unwrap().position.x - start.x,
        100 * scout_speed
    );
}

#[test]
fn replay_includes_economy_orders_carries_buildings_and_training() {
    fn run() -> [u8; 32] {
        let (mut world, node, villager) = resource_world(ResourceKind::Wood);
        world
            .initialize_village(PlayerId(1), TileCoord::new(64, 64))
            .unwrap();
        let center = world.buildings().next().unwrap().id;
        world.issue_gather(PlayerId(0), villager, node.id).unwrap();
        world
            .issue_train(PlayerId(1), center, UnitKind::Villager)
            .unwrap();
        advance(&mut world, 3000);
        world.canonical_hash()
    }
    assert_eq!(run(), run());
    let (mut a, _, center) = village();
    let (b, _, _) = village();
    a.issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    assert_ne!(a.canonical_hash(), b.canonical_hash());
}
