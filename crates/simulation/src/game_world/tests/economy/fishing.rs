use super::*;
use aoe_map::{ObjectKind, ordered_water_page_root};

fn coast() -> GameWorld {
    let (mut package, pages, mut water) = fixture::flat_fixture(true);
    package.request.requested_side_meters = 15_360;
    water[0].ocean_coverage_percent = vec![0, 100, 0, 100];
    water[1].ocean_coverage_percent = vec![50];
    for (level, page) in package
        .environment
        .water
        .as_mut()
        .unwrap()
        .levels
        .iter_mut()
        .zip(&water)
    {
        level.ordered_page_root = ordered_water_page_root(std::slice::from_ref(page)).unwrap();
    }
    let package = aoe_map::MapPackage::with_prepared_environment(
        1,
        package.request,
        Vec::new(),
        package.projection,
        package.provenance,
        package.environment,
    )
    .unwrap();
    GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap()
}

fn site(world: &GameWorld) -> TileCoord {
    let width = world.config().width_tiles;
    for y in 20..100 {
        for x in 4..width - 4 {
            let tile = TileCoord::new(x, y);
            if world
                .validate_building_placement(BuildingKind::Dock, tile)
                .is_ok()
            {
                return tile;
            }
        }
    }
    panic!("coast needs a dock site");
}

fn build_dock(world: &mut GameWorld, origin: TileCoord) -> EntityId {
    let mut worker = None;
    for y in origin.y - 1..=origin.y + 3 {
        for x in origin.x - 1..=origin.x + 3 {
            let tile = TileCoord::new(x, y);
            if x >= origin.x && x < origin.x + 3 && y >= origin.y && y < origin.y + 3 {
                continue;
            }
            if let Ok(id) =
                world.spawn_villager(PlayerId(0), WorldPosition::from_tile_center(tile).unwrap())
            {
                worker = Some(id);
                break;
            }
        }
        if worker.is_some() {
            break;
        }
    }
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .stockpile
        .wood = 1000;
    let dock = world
        .issue_build(PlayerId(0), &[worker.unwrap()], BuildingKind::Dock, origin)
        .unwrap();
    advance(world, 36 * 20);
    assert!(world.building(dock).unwrap().is_complete());
    dock
}

#[test]
fn dock_and_fishing_ship_reveal_their_los_and_enemy_ships_hide_after_sight_leaves() {
    use crate::TileVisibility::{Explored, Unexplored, Visible};
    let mut world = coast();
    world
        .place_town_center(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    let origin = site(&world);
    let dock = build_dock(&mut world, origin);
    assert_eq!(world.building(dock).unwrap().los, 8);
    let center = TileCoord::new(origin.x + 1, origin.y + 1);
    assert_eq!(
        world.visibility(PlayerId(0), TileCoord::new(center.x + 8, center.y)),
        Visible
    );
    assert_eq!(
        world.visibility(PlayerId(0), TileCoord::new(center.x + 9, center.y)),
        Unexplored
    );
    let tile = TileCoord::new(origin.x + 30, origin.y + 20);
    let at = WorldPosition::from_tile_center(tile).unwrap();
    let own = world
        .spawn_kind(PlayerId(0), UnitKind::FishingShip, at)
        .unwrap();
    let enemy = world
        .spawn_kind(PlayerId(1), UnitKind::FishingShip, at)
        .unwrap();
    assert_eq!(
        world.visibility(PlayerId(0), TileCoord::new(tile.x + 5, tile.y)),
        Visible
    );
    assert_eq!(
        world.visibility(PlayerId(0), TileCoord::new(tile.x + 6, tile.y)),
        Unexplored
    );
    assert!(world.unit_visible_to(PlayerId(0), enemy));
    world.kill_unit(world.lookup[&own]);
    assert_eq!(world.visibility(PlayerId(0), tile), Explored);
    assert!(!world.unit_visible_to(PlayerId(0), enemy));
}

#[test]
fn fish_keep_the_last_observed_amount_in_explored_fog() {
    use crate::TileVisibility;
    let mut world = coast();
    let crate::Terrain::Map { generator, .. } = world.terrain() else {
        panic!()
    };
    let fish = (0..5)
        .flat_map(|cy| (0..8).flat_map(move |cx| generator.chunk(cx, cy).unwrap().resources))
        .find(|n| n.object == ObjectKind::DeepFish)
        .unwrap();
    let at = WorldPosition::from_tile_center(TileCoord::new(fish.tile.x + 1, fish.tile.y)).unwrap();
    let ship = world
        .spawn_kind(PlayerId(0), UnitKind::FishingShip, at)
        .unwrap();
    world.deplete_resource(fish.id, 1).unwrap();
    assert_eq!(
        world.known_resource_amounts(PlayerId(0)),
        vec![(fish.id, fish.initial_amount - 1)]
    );
    world.kill_unit(world.lookup[&ship]);
    assert_eq!(
        world.visibility(PlayerId(0), fish.tile),
        TileVisibility::Explored
    );
    world.deplete_resource(fish.id, 1).unwrap();
    assert_eq!(
        world.known_resource_amounts(PlayerId(0)),
        vec![(fish.id, fish.initial_amount - 1)]
    );
    world
        .spawn_kind(PlayerId(0), UnitKind::FishingShip, at)
        .unwrap();
    assert_eq!(
        world.known_resource_amounts(PlayerId(0)),
        vec![(fish.id, fish.initial_amount - 2)]
    );
}

#[test]
fn dock_rejects_inland_and_open_water_but_accepts_a_coast() {
    let mut world = coast();
    assert!(
        world
            .validate_building_placement(BuildingKind::Dock, TileCoord::new(10, 10))
            .is_err()
    );
    let width = world.config().width_tiles;
    assert!(
        world
            .validate_building_placement(BuildingKind::Dock, TileCoord::new(width - 10, 10))
            .is_err()
    );
    let origin = site(&world);
    assert!(
        world
            .validate_building_placement(BuildingKind::Barracks, origin)
            .is_err()
    );
    let rules = BuildingKind::Dock.rules();
    assert_eq!(
        (
            rules.cost.wood,
            rules.side,
            rules.build_seconds,
            rules.max_hp
        ),
        (150, 3, 35, 1800)
    );
    let center = world
        .place_town_center(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    let dock = build_dock(&mut world, origin);
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .stockpile
        .wood = 150;
    assert!(
        world
            .issue_train(PlayerId(0), center, UnitKind::FishingShip)
            .is_err()
    );
    world
        .issue_train(PlayerId(0), dock, UnitKind::FishingShip)
        .unwrap();
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().stockpile.wood,
        75
    );
    advance(&mut world, 40 * 20);
    let ship = world
        .units()
        .find(|u| u.kind == UnitKind::FishingShip)
        .unwrap();
    assert!(
        !world
            .terrain()
            .passable(ship.position.tile_floor(), world.config())
    );
    assert!(
        world
            .issue_move(
                ship.id,
                WorldPosition::from_tile_center(TileCoord::new(20, 20)).unwrap()
            )
            .is_err()
    );
    let target = TileCoord::new(origin.x + 20, origin.y + 10);
    world
        .issue_move(ship.id, WorldPosition::from_tile_center(target).unwrap())
        .unwrap();
    advance(&mut world, 60 * 20);
    assert_eq!(world.unit(ship.id).unwrap().position.tile_floor(), target);
}

#[test]
fn shore_fisher_and_deep_fishing_ship_gather_and_deliver_at_dat_rates() {
    let mut world = coast();
    world
        .place_town_center(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    let origin = site(&world);
    let dock = build_dock(&mut world, origin);
    let crate::Terrain::Map { generator, .. } = world.terrain() else {
        panic!()
    };
    let nodes = (0..5)
        .flat_map(|cy| (0..8).flat_map(move |cx| generator.chunk(cx, cy).unwrap().resources))
        .collect::<Vec<_>>();
    for (object, kind, expected, capacity) in [
        (ObjectKind::ShoreFish, UnitKind::Villager, 4300, 10),
        (ObjectKind::DeepFish, UnitKind::FishingShip, 4165, 15),
        (ObjectKind::ShoreFish, UnitKind::FishingShip, 2380, 15),
    ] {
        let node = nodes
            .iter()
            .find(|n| n.object == object && n.tile.y > 8)
            .unwrap();
        let mut worker = None;
        for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            let tile = TileCoord::new(node.tile.x + dx, node.tile.y + dy);
            if let Ok(id) = world.spawn_kind(
                PlayerId(0),
                kind,
                WorldPosition::from_tile_center(tile).unwrap(),
            ) {
                worker = Some(id);
                break;
            }
        }
        let id = worker.expect("gather approach");
        world.issue_gather(PlayerId(0), id, node.id).unwrap();
        advance(&mut world, 20 * 11);
        let worker_tile = world.unit(id).unwrap().position.tile_floor();
        let facing = match (node.tile.x - worker_tile.x, node.tile.y - worker_tile.y) {
            (0, 1) => 7,
            (1, 0) => 1,
            (-1, 0) => 5,
            (0, -1) => 3,
            _ => panic!("worker must stand beside the fish"),
        };
        assert_eq!(world.unit(id).unwrap().facing as u8, facing);
        assert_eq!(
            world.unit(id).unwrap().carried.unwrap().amount,
            expected * 11 / 10_000
        );
        let before = world.player_economy(PlayerId(0)).unwrap().stockpile.food;
        advance(&mut world, 20 * 240);
        assert!(world.player_economy(PlayerId(0)).unwrap().stockpile.food >= before + capacity);
        world
            .issue_move(id, world.unit(id).unwrap().position)
            .unwrap();
    }
    assert!(world.building(dock).unwrap().is_complete());
}

#[test]
fn naval_routes_on_a_hundred_thousand_tile_map_keep_incremental_caps_and_replay() {
    let run = || {
        let (base, pages, water) = fixture::flat_fixture(true);
        let package = aoe_map::MapPackage::with_prepared_environment(
            1,
            aoe_map::MapRequest {
                requested_side_meters: 6_000_000,
                ..base.request
            },
            Vec::new(),
            base.projection,
            base.provenance,
            base.environment,
        )
        .unwrap();
        let mut world =
            GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
        let start = (10..20)
            .map(|x| TileCoord::new(x, 10))
            .find(|t| {
                world
                    .unit_tile_passable(UnitKind::FishingShip, *t, &|| false)
                    .unwrap()
            })
            .unwrap();
        let ship = world
            .spawn_kind(
                PlayerId(0),
                UnitKind::FishingShip,
                WorldPosition::from_tile_center(start).unwrap(),
            )
            .unwrap();
        let end = (80_000..80_010)
            .map(|x| TileCoord::new(x, 80_000))
            .find(|t| {
                world
                    .unit_tile_passable(UnitKind::FishingShip, *t, &|| false)
                    .unwrap()
            })
            .unwrap();
        world
            .issue_move(ship, WorldPosition::from_tile_center(end).unwrap())
            .unwrap();
        for _ in 0..100 {
            world.advance();
            assert!(world.active_planner_count <= crate::game_movement::MAX_ACTIVE_ROUTE_PLANNERS);
            assert!(world.active_route_searches <= crate::game_movement::MAX_ACTIVE_ROUTE_SEARCHES);
        }
        let unit = world.unit(ship).unwrap();
        assert!(unit.moving || unit.planning);
        assert!(unit.position.tile_floor().x.abs_diff(start.x) <= 7);
        world.canonical_hash()
    };
    assert_eq!(run(), run());
}
