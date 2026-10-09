use super::*;
use crate::GameWorld;
use aoe_core::{PlayerId, WorldPosition};
use aoe_map::MovementOutcome;

#[test]
fn paris_procedural_preview_uses_the_actual_runtime_start_certificate() {
    let request = aoe_map::MapRequest {
        schema_version: 1,
        center_latitude_e7: 488566000,
        center_longitude_e7: 23522000,
        requested_side_meters: 30720,
        compression: aoe_map::Ratio::new(30, 1).expect("compression"),
        year_ce: 600,
        seed: 1,
        reconstruction_profile: aoe_map::ReconstructionProfile::Circa600V1,
        detail_profile: aoe_map::DetailProfile::LandscapeV2,
    };
    // Match /maps/activate: no prepared pages, no source locks, no flat fixture.
    let package = aoe_map::MapPackage::new(aoe_map::MAP_SCHEMA_VERSION, request, Vec::new())
        .expect("fallback package");
    assert_eq!(package.schema_version, 10);
    assert_eq!(package.generation_recipe_version, LANDSCAPE_START_RECIPE);
    assert_eq!(package.environment.samples_per_axis, 0);
    assert!(package.source_locks.is_empty());
    let world = GameWorld::from_map(package).expect("runtime fallback world");
    let config = world.config();
    assert_eq!((config.width_tiles, config.height_tiles), (512, 512));
    let result = world
        .terrain()
        .search_start_for_recipe(config, LANDSCAPE_START_RECIPE, 64, || false)
        .expect("fallible actual start certificate");
    // This exact unsupported fallback exhausts the unchanged certificate bound.
    // Never fabricate a scout, flatten relief, or borrow the prepared-field result.
    assert_eq!(result, StartSearchResult::LimitReached);
    assert_eq!(
        world
            .terrain()
            .search_start_for_recipe(config, LANDSCAPE_START_RECIPE, 64, || true,),
        Ok(StartSearchResult::Cancelled),
    );
}

#[test]
fn composed_forest_start_has_bounded_exit_and_replayable_cavalry_route() {
    let generator = flat_temperate_generator(LANDSCAPE_START_RECIPE);
    let config = crate::movement_speed::cavalry_config(
        WorldConfig::new(512, 512, Seed(600)).expect("config"),
    );
    let start = TileCoord::new(255, 255);
    let mut targets = Vec::new();
    for dy in -1..=1 {
        for dx in -1..=1 {
            let target = generator
                .forest_opening_center_at(TileCoord::new(start.x + dx * 192, start.y + dy * 192))
                .expect("opening");
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
    let mut world = GameWorld::new(config).expect("world");
    world.terrain = Terrain::Map {
        generator,
        overlay: ResourceOverlay::default(),
    };
    assert_eq!(
        world
            .terrain
            .search_start_for_recipe(config, LANDSCAPE_START_RECIPE, 64, || false),
        Ok(StartSearchResult::Found(start))
    );
    let mut cache = StartPassabilityCache::new(&world.terrain, config, &|| false);
    assert!(cache.reaches_exit(start).expect("exit certificate"));
    assert!(cache.exit_visits <= START_EXIT_VISITS);
    assert!(cache.exit_chunks.len() <= START_SEARCH_CHUNKS);
    let mut failures = Vec::new();
    let reached = targets.into_iter().find_map(|target| {
        match world.terrain.route_outcome_with_limit(start, target, 4_096) {
            Some(MovementOutcome::Path(path)) => Some((target, path.tiles)),
            outcome => {
                failures.push((target, outcome));
                None
            }
        }
    });
    let (target, route) =
        reached.unwrap_or_else(|| panic!("bounded route to another opening: {failures:?}"));
    for edge in route.windows(2) {
        assert!(world.terrain.crossable(edge[0], edge[1], config));
    }
    let position = WorldPosition::from_tile_center(start).expect("start position");
    let destination = WorldPosition::from_tile_center(target).expect("destination");
    let unit = world.spawn_unit(PlayerId(0), position).expect("cavalry");
    world.issue_move(unit, destination).expect("order");
    let mut replay = GameWorld::new(config).expect("replay");
    replay.terrain = Terrain::Map {
        generator: flat_temperate_generator(LANDSCAPE_START_RECIPE),
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
}
