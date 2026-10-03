use super::*;
use crate::GameWorld;
use aoe_core::{PlayerId, WorldPosition};
use aoe_map::MovementOutcome;

#[test]
fn certified_recipe_eight_cavalry_leaves_glade_and_reaches_another_opening() {
    let generator = flat_temperate_generator(WATER_MODEL_START_RECIPE);
    let config = crate::movement_speed::cavalry_config(
        WorldConfig::new(512, 512, Seed(600)).expect("config"),
    );
    let start = TileCoord::new(255, 255);
    let mut world = GameWorld::new(config).expect("world");
    let mut targets = Vec::new();
    for dy in -1..=1 {
        for dx in -1..=1 {
            let target = generator
                .forest_opening_center_at(TileCoord::new(start.x + dx * 192, start.y + dy * 192))
                .expect("node");
            if target.x.abs_diff(start.x).max(target.y.abs_diff(start.y)) >= START_EXIT_DISTANCE {
                targets.push(target);
            }
        }
    }
    targets.sort_by_key(|target| {
        (
            target.x.abs_diff(start.x).pow(2) + target.y.abs_diff(start.y).pow(2),
            target.y,
            target.x,
        )
    });
    world.terrain = Terrain::Map {
        generator,
        overlay: ResourceOverlay::default(),
    };
    assert_eq!(
        world
            .terrain
            .search_start_for_recipe(config, WATER_MODEL_START_RECIPE, 64, || false),
        Ok(StartSearchResult::Found(start))
    );
    let mut cache = StartPassabilityCache::new(&world.terrain, config, &|| false);
    assert!(cache.reaches_exit(start).expect("exit certificate"));
    assert!(cache.exit_visits <= START_EXIT_VISITS);
    assert!(cache.exit_chunks.len() <= START_SEARCH_CHUNKS);
    let (target, route) = targets
        .into_iter()
        .find_map(
            |target| match world.terrain.route_outcome_with_limit(start, target, 4_096) {
                Some(MovementOutcome::Path(path)) => Some((target, path.tiles)),
                _ => None,
            },
        )
        .expect("bounded route to another opening");
    for edge in route.windows(2) {
        assert!(world.terrain.crossable(edge[0], edge[1], config));
    }
    let position = WorldPosition::from_tile_center(start).expect("start");
    let unit = world.spawn_unit(PlayerId(0), position).expect("cavalry");
    let destination = WorldPosition::from_tile_center(target).expect("target");
    world.issue_move(unit, destination).expect("move order");
    let mut replay = GameWorld::new(config).expect("replay");
    replay.terrain = Terrain::Map {
        generator: flat_temperate_generator(WATER_MODEL_START_RECIPE),
        overlay: ResourceOverlay::default(),
    };
    let repeated = replay
        .spawn_unit(PlayerId(0), position)
        .expect("replay cavalry");
    replay
        .issue_move(repeated, destination)
        .expect("replay order");
    for _ in 0..4_096 {
        world.advance();
        replay.advance();
        assert_eq!(world.canonical_hash(), replay.canonical_hash());
        if world.unit(unit).expect("unit").position == destination {
            break;
        }
    }
    assert_eq!(world.unit(unit).expect("unit").position, destination);
    assert!(target.x.abs_diff(start.x).max(target.y.abs_diff(start.y)) > 27);
}

#[test]
fn source_water_is_not_cleared_to_certify_recipe_eight_start() {
    let config = WorldConfig::new(64, 64, Seed(1)).expect("config");
    let terrain = Terrain::Map {
        generator: flat_temperate_generator_with_water(WATER_MODEL_START_RECIPE, 100),
        overlay: ResourceOverlay::default(),
    };
    assert_eq!(
        terrain.search_start_for_recipe(config, WATER_MODEL_START_RECIPE, 64, || false),
        Ok(StartSearchResult::Unavailable)
    );
    assert!(!terrain.passable(TileCoord::new(31, 31), config));
}

#[test]
fn small_recipe_eight_worlds_certify_a_bounded_route_toward_the_edge() {
    let terrain = Terrain::uniform(1);
    for width in [32, 64, 128, 512] {
        let config = WorldConfig::new(width, width, Seed(1)).expect("config");
        let center = TileCoord::new((width - 1) / 2, (width - 1) / 2);
        assert_eq!(
            terrain.search_start_for_recipe(config, WATER_MODEL_START_RECIPE, 64, || false),
            Ok(StartSearchResult::Found(center))
        );
        let mut cache = StartPassabilityCache::new(&terrain, config, &|| false);
        assert!(cache.reaches_exit(center).expect("bounded exit"));
        assert!(cache.exit_visits <= START_EXIT_VISITS);
        assert!(cache.exit_chunks.len() <= START_SEARCH_CHUNKS);
    }
}

#[test]
fn isolated_large_world_glade_is_not_a_certified_recipe_eight_start() {
    let config = WorldConfig::new(512, 512, Seed(1)).expect("config");
    let center = TileCoord::new(255, 255);
    // Four central source cells cover a roughly 33-tile land island in ocean.
    // Its legacy component exceeds 256, but no point can reach 64 tiles away.
    // A power-of-two axis has a complete 32/16/8/4/2/1 verified mip chain.
    let mut coverage = vec![100; 32 * 32];
    for y in 15..=16 {
        for x in 15..=16 {
            coverage[y * 32 + x] = 0;
        }
    }
    let terrain = Terrain::Map {
        generator: flat_temperate_generator_with_water_field(
            WATER_MODEL_START_RECIPE,
            32,
            coverage,
        ),
        overlay: ResourceOverlay::default(),
    };
    let mut cache = StartPassabilityCache::new(&terrain, config, &|| false);
    assert!(clear_starting_area(&mut cache, center).expect("footprint"));
    assert!(
        cache
            .reaches_required_tiles(center)
            .expect("legacy component")
    );
    assert!(
        !cache
            .reaches_exit(center)
            .expect("isolated source component")
    );
    assert!(cache.exit_visits <= START_EXIT_VISITS);
    assert!(cache.exit_chunks.len() <= START_SEARCH_CHUNKS);
    assert!(!matches!(
        terrain.search_start_for_recipe(config, WATER_MODEL_START_RECIPE, 64, || false),
        Ok(StartSearchResult::Found(_))
    ));
    assert_eq!(
        terrain.search_start_for_recipe(config, PRIOR_FOREST_START_RECIPE, 64, || false),
        Ok(StartSearchResult::Found(center))
    );
}
