use super::*;
use aoe_core::{Seed, TILE_GROUND_RADIUS_SUBUNITS};
use aoe_map::{MapChunkGenerator, MapPackage, MapRequest, ResourceOverlay};

fn world() -> GameWorld {
    GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap()
}

#[test]
fn fixed_speed_has_equal_distance_for_cardinal_and_diagonal_orders() {
    let mut cardinal = world();
    let mut diagonal = world();
    let start = WorldPosition::new(10 * FIXED_SUBUNITS_PER_TILE, 10 * FIXED_SUBUNITS_PER_TILE);
    let a = cardinal.spawn_unit(PlayerId(0), start).unwrap();
    let b = diagonal.spawn_unit(PlayerId(0), start).unwrap();
    cardinal
        .issue_move(a, WorldPosition::new(start.x + 5_000, start.y))
        .unwrap();
    diagonal
        .issue_move(b, WorldPosition::new(start.x + 5_000, start.y + 5_000))
        .unwrap();
    assert_eq!(cardinal.movement_order(a).unwrap().travelled, 0);
    for _ in 0..8 {
        cardinal.advance();
        diagonal.advance();
    }
    assert_eq!(cardinal.movement_order(a).unwrap().travelled, 1_024);
    assert_eq!(diagonal.movement_order(b).unwrap().travelled, 1_024);
}

#[test]
fn exact_arrival_and_mid_move_redirection_are_deterministic() {
    let mut first = world();
    let mut second = world();
    let start = WorldPosition::new(20_000, 20_000);
    let a = first.spawn_unit(PlayerId(0), start).unwrap();
    let b = second.spawn_unit(PlayerId(0), start).unwrap();
    let target = WorldPosition::new(23_000, 24_000);
    first.issue_move(a, target).unwrap();
    second.issue_move(b, target).unwrap();
    for _ in 0..100 {
        first.advance();
        second.advance();
    }
    assert_eq!(
        first.unit(a),
        second.unit(b).map(|unit| GameUnit { id: a, ..unit })
    );
    assert_eq!(
        first.unit(a).unwrap().position,
        WorldConfig::default().snap_ground_position(target)
    );
    assert!(!first.unit(a).unwrap().moving);
    first
        .issue_move(a, WorldPosition::new(25_000, 24_000))
        .unwrap();
    first.advance();
    assert!(first.unit(a).unwrap().moving);
}

#[test]
fn move_destinations_snap_to_tile_centers() {
    let mut world = world();
    let unit = world
        .spawn_unit(PlayerId(0), WorldPosition::new(20_000, 20_000))
        .unwrap();
    world
        .issue_move(unit, WorldPosition::new(23_000, 24_000))
        .unwrap();
    assert_eq!(
        world.movement_order(unit).unwrap().destination,
        WorldConfig::default().snap_ground_position(WorldPosition::new(23_000, 24_000))
    );
    assert_ne!(
        world.movement_order(unit).unwrap().waypoint,
        world.movement_order(unit).unwrap().destination
    );
}

#[test]
fn move_to_current_tile_center_cancels_without_active_work() {
    let mut world = world();
    let center = WorldPosition::from_tile_center(TileCoord::new(20, 20)).unwrap();
    let unit = world.spawn_unit(PlayerId(0), center).unwrap();
    assert!(!world.issue_move(unit, center).unwrap());
    assert_eq!(world.active_mover_count(), 0);
    assert!(!world.unit(unit).unwrap().moving);
}

#[test]
fn facing_covers_all_isometric_direction_bands() {
    assert_eq!(facing_for(0, 0, Facing::North), Facing::North);
    assert_eq!(facing_for(10, -10, Facing::North), Facing::East);
    assert_eq!(facing_for(-10, 10, Facing::North), Facing::West);
    assert_eq!(facing_for(10, 10, Facing::North), Facing::South);
    assert_eq!(facing_for(-10, -10, Facing::South), Facing::North);
    assert_eq!(facing_for(1, 0, Facing::North), Facing::SouthEast);
    assert_eq!(facing_for(0, -1, Facing::North), Facing::NorthEast);
    assert_eq!(facing_for(0, 1, Facing::North), Facing::SouthWest);
    assert_eq!(facing_for(-1, 0, Facing::North), Facing::NorthWest);
}

#[test]
fn unknown_orders_and_hash_hex_are_deterministic() {
    let mut world = world();
    assert_eq!(
        world.issue_move(EntityId(999), WorldPosition::new(1, 1)),
        Err(GameWorldError::UnknownEntity)
    );
    assert_eq!(world.canonical_hash_hex().len(), 64);
}

#[test]
fn default_cavalry_starts_at_a_tile_center() {
    let (world, unit) = GameWorld::default_with_cavalry(Seed(7));
    let position = world.unit(unit).unwrap().position;
    assert_eq!(position.x.rem_euclid(FIXED_SUBUNITS_PER_TILE), 512);
    assert_eq!(position.y.rem_euclid(FIXED_SUBUNITS_PER_TILE), 512);
}

#[test]
fn convenience_world_constructors_build_the_requested_worlds() {
    let config = WorldConfig::new(256, 256, Seed(7)).unwrap();
    let (world, unit) = GameWorld::with_cavalry(config).unwrap();
    assert_eq!(world.unit_count(), 1);
    assert!(world.unit_exists(unit));
    let populated = GameWorld::with_population(config, 4, 1, 2).unwrap();
    assert_eq!(populated.unit_count(), 4);
}

#[test]
fn swap_removal_updates_displaced_bucket_slot() {
    let mut world = world();
    let first = world
        .spawn_unit(PlayerId(0), WorldPosition::new(32_000, 32_000))
        .unwrap();
    let second = world
        .spawn_unit(PlayerId(0), WorldPosition::new(32_100, 32_000))
        .unwrap();
    world
        .issue_move(first, WorldPosition::new(65_000, 32_000))
        .unwrap();
    world.advance();
    world
        .issue_move(second, WorldPosition::new(32_200, 32_000))
        .unwrap();
    let (units, _) = world.query(TileRect::from_xywh(31, 31, 40, 40));
    assert!(units.iter().any(|unit| unit.id == second));
}

#[test]
fn queries_are_stable_and_idle_units_do_not_enter_movement_work() {
    let mut world = world();
    let first = world
        .spawn_unit(
            PlayerId(0),
            WorldPosition::new(TILE_GROUND_RADIUS_SUBUNITS, TILE_GROUND_RADIUS_SUBUNITS),
        )
        .unwrap();
    let second = world
        .spawn_unit(PlayerId(0), WorldPosition::new(10_000, 10_000))
        .unwrap();
    let (before, stats) = world.query(TileRect::from_xywh(0, 0, 8, 8));
    assert_eq!(before[0].id, first);
    assert_eq!(stats.returned_units, 1);
    assert_eq!(world.active_mover_count(), 0);
    world
        .issue_move(second, WorldPosition::new(11_000, 10_000))
        .unwrap();
    assert_eq!(world.active_mover_count(), 1);
    let hash = world.canonical_hash();
    world.advance();
    assert_ne!(world.canonical_hash(), hash);
}

#[test]
fn dimensions_do_not_allocate_empty_map_storage() {
    let small = GameWorld::new(WorldConfig::new(1_024, 1_024, Seed(1)).unwrap()).unwrap();
    let large = GameWorld::new(WorldConfig::new(16_384, 16_384, Seed(1)).unwrap()).unwrap();
    assert_eq!(small.unit_count(), large.unit_count());
    assert_eq!(small.occupied_chunk_count(), large.occupied_chunk_count());
}

#[test]
fn deterministic_population_supports_hotspots_and_sparse_extents() {
    let config = WorldConfig::new(16_384, 16_384, Seed(19)).unwrap();
    let world = GameWorld::with_population_in_extent(config, 8_000, 1_000, 4, 1_024).unwrap();
    let (units, stats) = world.query(TileRect::from_xywh(0, 0, 128, 128));
    assert_eq!(world.unit_count(), 8_000);
    assert!(units.len() >= 1_000);
    assert!(stats.candidate_units >= stats.returned_units);
}

#[test]
fn map_world_rejects_blocked_ground_before_spawning_or_ordering() {
    let package = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
    let mut world = GameWorld::from_map(package).expect("map world");
    let config = world.config();
    let mut passable = None;
    let mut blocked = None;
    for y in 0..config.height_tiles {
        for x in 0..config.width_tiles {
            let tile = TileCoord::new(x, y);
            if world.terrain().passable(tile, config) {
                passable.get_or_insert(tile);
            } else {
                blocked.get_or_insert(tile);
            }
            if passable.is_some() && blocked.is_some() {
                break;
            }
        }
        if passable.is_some() && blocked.is_some() {
            break;
        }
    }
    let unit = world
        .spawn_unit(
            PlayerId(0),
            WorldPosition::from_tile_center(passable.expect("land")).expect("position"),
        )
        .expect("spawn on land");
    let blocked =
        WorldPosition::from_tile_center(blocked.expect("blocked tile")).expect("position");
    assert_eq!(
        world.issue_move(unit, blocked),
        Err(GameWorldError::InvalidPosition)
    );
}

#[test]
fn exhausting_a_resource_releases_its_blocking_tile() {
    let config = WorldConfig::new(128, 128, Seed(3)).expect("config");
    let generator = MapChunkGenerator::new([3; 32], 1, config.width_tiles);
    let resource = (0..4)
        .flat_map(|y| {
            let generator = generator.clone();
            (0..4).flat_map(move |x| generator.chunk(x, y).resources)
        })
        .next()
        .expect("resource");
    let mut world = GameWorld::new(config).expect("world");
    world.terrain = Terrain::Map {
        generator,
        overlay: ResourceOverlay::default(),
    };
    assert!(!world.terrain().passable(resource.tile, config));
    assert!(
        world
            .next_map_route(TileCoord::new(0, 0), TileCoord::new(0, 0))
            .is_some()
    );
    assert_eq!(world.navigation_cache.entry_count(), 1);
    let first = world
        .deplete_resource(resource.id, resource.initial_amount - 1)
        .expect("partial depletion");
    assert!(first.remaining > 0);
    assert!(!first.became_nonblocking);
    assert_eq!(world.navigation_cache.entry_count(), 1);
    let final_depletion = world
        .deplete_resource(resource.id, resource.initial_amount)
        .expect("final depletion");
    assert_eq!(final_depletion.remaining, 0);
    assert!(final_depletion.became_nonblocking);
    assert_eq!(world.navigation_cache.entry_count(), 0);
    assert!(world.terrain().passable(resource.tile, config));
}

#[test]
fn map_world_follows_a_passable_route_to_its_destination() {
    let config = WorldConfig::new(64, 64, Seed(0)).expect("config");
    let generator = MapChunkGenerator::new([0; 32], 0, config.width_tiles);
    let terrain = Terrain::Map {
        generator: generator.clone(),
        overlay: ResourceOverlay::default(),
    };
    let mut route_fixture = None;
    'origins: for y in 1..config.height_tiles - 1 {
        for x in 1..config.width_tiles - 2 {
            let origin = TileCoord::new(x, y);
            let destination = TileCoord::new(x + 1, y);
            if let Some(path) = terrain.route(origin, destination) {
                route_fixture = Some((origin, destination, path));
                break 'origins;
            }
        }
    }
    let (origin, destination, path) = route_fixture.expect("generated terrain has a route");
    let mut world = GameWorld::new(config).expect("world");
    world.terrain = Terrain::Map {
        generator,
        overlay: ResourceOverlay::default(),
    };
    let unit = world
        .spawn_unit(
            PlayerId(0),
            WorldPosition::from_tile_center(origin).expect("origin"),
        )
        .expect("spawn");
    let target = WorldPosition::from_tile_center(destination).expect("destination");
    assert!(world.issue_move(unit, target).expect("route"));
    assert_eq!(
        world.movement_order(unit).expect("order").waypoint,
        WorldPosition::from_tile_center(path[1]).expect("first route waypoint")
    );
    for _ in 0..512 {
        for changed in world.advance() {
            assert!(
                world
                    .terrain()
                    .passable(changed.position.tile_floor(), config)
            );
        }
        if !world.unit(unit).expect("unit").moving {
            break;
        }
    }
    let arrived = world.unit(unit).expect("unit");
    assert_eq!(arrived.position, target);
    assert!(!arrived.moving);
}

#[test]
fn exhausted_global_planning_budget_defers_a_map_segment_instead_of_moving() {
    let config = WorldConfig::new(64, 64, Seed(0)).expect("config");
    let generator = MapChunkGenerator::new([0; 32], 0, config.width_tiles);
    let terrain = Terrain::Map {
        generator: generator.clone(),
        overlay: ResourceOverlay::default(),
    };
    let mut fixture = None;
    'origins: for y in 1..config.height_tiles - 1 {
        for x in 1..config.width_tiles - 3 {
            let origin = TileCoord::new(x, y);
            let destination = TileCoord::new(x + 2, y);
            if let Some(MovementOutcome::Path(path)) = terrain.route_outcome(origin, destination)
                && path.tiles.len() >= 3
            {
                fixture = Some((origin, destination, path.tiles[1]));
                break 'origins;
            }
        }
    }
    let (origin, destination, waypoint) = fixture.expect("generated map route");
    let mut world = GameWorld::new(config).expect("world");
    world.terrain = Terrain::Map {
        generator,
        overlay: ResourceOverlay::default(),
    };
    let id = world
        .spawn_unit(
            PlayerId(0),
            WorldPosition::from_tile_center(origin).expect("origin"),
        )
        .expect("spawn");
    let index = world.lookup[&id];
    let origin_position = WorldPosition::from_tile_center(origin).expect("origin position");
    let waypoint_position = WorldPosition::from_tile_center(waypoint).expect("waypoint position");
    let destination_position =
        WorldPosition::from_tile_center(destination).expect("destination position");
    world.units[index].state.moving = true;
    world.units[index].order = Some(MovementOrder {
        origin: origin_position,
        destination: destination_position,
        waypoint: waypoint_position,
        target_tile: destination,
        segment_length: 1,
        travelled: 1,
    });
    world.active_movers.push(id);
    world.planning_budget = 0;

    world.advance();

    let unit = world.unit(id).expect("unit");
    assert_eq!(unit.position, waypoint_position);
    assert!(!unit.moving);
    assert!(unit.planning);
    assert!(world.movement_order(id).is_some());
    assert_eq!(world.active_mover_count(), 1);
}
