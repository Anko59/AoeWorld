mod barracks;
mod castle;
mod feudal;
mod gates;
mod research;
use super::*;
use crate::{BuildingKind, CarriedResource, EconomyError, UnitKind, UnitTask, VILLAGE_RULES};
use aoe_map::{ResourceKind, ResourceNode};

#[path = "economy/navigation/bridges.rs"]
mod bridges;
mod buildings;
mod construction;
mod farming;
mod fishing;
pub(crate) mod fixture;
mod gathering;
mod market;
mod rejections;
#[path = "economy/navigation/resource_collision.rs"]
mod resource_collision;
mod training;
#[path = "economy/navigation/village_routes.rs"]
mod village_routes;

fn village() -> (GameWorld, EntityId, EntityId) {
    let mut world = world();
    let scout = world
        .initialize_village(PlayerId(0), TileCoord::new(64, 64))
        .unwrap();
    let center = world.buildings().next().unwrap().id;
    (world, scout, center)
}

fn resource_world(kind: ResourceKind) -> (GameWorld, ResourceNode, EntityId) {
    let (package, pages, water) = fixture::flat_fixture(false);
    let generator = package
        .generator_with_environment(pages.clone(), water.clone(), Vec::new(), Vec::new())
        .unwrap();
    let mut world =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    for y in 0..8 {
        for x in 0..8 {
            for node in generator.chunk(x, y).unwrap().resources {
                if node.kind != kind {
                    continue;
                }
                if kind == ResourceKind::Wood
                    && world
                        .nearest_resource(node.tile, kind, 10, Some(node.id))
                        .is_none()
                {
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
            assert_eq!(
                world.tile_passable(TileCoord::new(x, y)),
                !(x < building.footprint.min.x + 2 && y < building.footprint.min.y + 2)
            );
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
fn movement_routes_around_the_solid_center_annex_and_rejects_its_interior() {
    let (mut world, scout, center) = village();
    let footprint = world.building(center).unwrap().footprint;
    let solid = aoe_core::TileRect::from_xywh(footprint.min.x, footprint.min.y, 2, 2);
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
        assert!(!solid.contains(world.unit(scout).unwrap().position.tile_floor()));
    }
    assert_eq!(world.unit(scout).unwrap().position.tile_floor(), target);
}

#[test]
fn villagers_move_at_point_eight_tiles_per_second_and_scout_keeps_its_speed() {
    let mut world = world();
    let start = WorldPosition::from_tile_center(TileCoord::new(20, 20)).unwrap();
    let villager = world.spawn_villager(PlayerId(0), start).unwrap();
    // Measure free speed on separate lanes. A stacked scout now yields to a
    // villager in front; persistent overlap is covered by the spacing tests.
    let scout_start = WorldPosition::from_tile_center(TileCoord::new(20, 24)).unwrap();
    let scout = world.spawn_unit(PlayerId(0), scout_start).unwrap();
    let destination = WorldPosition::from_tile_center(TileCoord::new(60, 20)).unwrap();
    world.issue_move(villager, destination).unwrap();
    world
        .issue_move(
            scout,
            WorldPosition::from_tile_center(TileCoord::new(60, 24)).unwrap(),
        )
        .unwrap();
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

#[test]
fn water_never_accepts_a_town_center_or_a_starting_village() {
    let (package, pages, water) = fixture::flat_fixture(true);
    let mut world =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    let before = world.canonical_hash();
    assert_eq!(
        world.place_town_center(PlayerId(0), TileCoord::new(100, 100)),
        Err(EconomyError::InvalidFootprint)
    );
    assert_eq!(world.canonical_hash(), before);
    assert!(
        world
            .initialize_village(PlayerId(0), TileCoord::new(100, 100))
            .is_err()
    );
    assert_eq!(world.canonical_hash(), before);
}

#[test]
fn a_certified_clearing_can_hold_the_center_while_the_scout_uses_its_perimeter() {
    let mut world = world();
    let start = TileCoord::new(64, 64);
    // A bounded impassable ring models the edge of the certified 5x5 clearing.
    for y in -3_i32..=3 {
        for x in -3_i32..=3 {
            if x.abs() == 3 || y.abs() == 3 {
                world
                    .economy
                    .blocked_tiles
                    .insert(TileCoord::new(start.x + x, start.y + y));
            }
        }
    }
    let scout = world.initialize_village(PlayerId(0), start).unwrap();
    let center = world.buildings().next().unwrap();
    assert!(center.footprint.contains(start));
    assert!(
        !center
            .footprint
            .contains(world.unit(scout).unwrap().position.tile_floor())
    );
    assert_eq!(scout, EntityId(0));
    assert_eq!(world.unit_count(), 4);
}
