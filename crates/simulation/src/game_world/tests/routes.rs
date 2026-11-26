use super::*;
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentPage, EnvironmentPageError,
    EnvironmentPageKey, EnvironmentPageProvider, FieldPyramid, PreparedEnvironment, PyramidLevel,
    Ratio, RoutePlannerPoll, WaterPage, ordered_page_root,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug)]
struct ToggleProvider {
    fail: Arc<AtomicBool>,
    page: Arc<EnvironmentPage>,
}

impl EnvironmentPageProvider for ToggleProvider {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if self.fail.load(Ordering::Relaxed) {
            return Err(EnvironmentPageError::Unavailable);
        }
        (self.page.key() == key)
            .then_some(self.page.clone())
            .ok_or(EnvironmentPageError::Missing)
    }
}

fn flat_map_terrain() -> Terrain {
    flat_map_terrain_size(64)
}

fn flat_map_terrain_size(width: i32) -> Terrain {
    let elevation = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![0; 4],
    };
    let elevation_overview = ElevationPage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    };
    let water = WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        ocean_coverage_percent: vec![0; 4],
        inland_coverage_percent: vec![0; 4],
    };
    let water_overview = WaterPage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        ocean_coverage_percent: vec![0],
        inland_coverage_percent: vec![0],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&elevation))
                        .expect("elevation root"),
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&elevation_overview))
                        .expect("elevation overview root"),
                },
            ],
        },
        water: Some(FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: aoe_map::ordered_water_page_root(std::slice::from_ref(
                        &water,
                    ))
                    .expect("water root"),
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: aoe_map::ordered_water_page_root(std::slice::from_ref(
                        &water_overview,
                    ))
                    .expect("water overview root"),
                },
            ],
        }),
        vegetation: None,
        historical_land_use: None,

        hydrology_evidence: None,
    };
    let generator = MapChunkGenerator::new([0; 32], 0, width)
        .with_prepared_elevation(
            Ratio::new(1, 1).expect("compression"),
            &environment,
            vec![elevation, elevation_overview],
        )
        .expect("elevation")
        .with_prepared_water(&environment, vec![water, water_overview])
        .expect("water");
    let mut overlay = ResourceOverlay::default();
    for y in 0..width {
        for x in 0..width {
            let tile = TileCoord::new(x, y);
            if let Some(node) = generator.object_at(tile) {
                overlay
                    .deplete(&generator, node.id, node.initial_amount)
                    .expect("resource");
            }
        }
    }
    Terrain::Map { generator, overlay }
}

fn toggle_provider_terrain() -> (Terrain, Arc<AtomicBool>) {
    let elevation = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![0; 4],
    };
    let overview = ElevationPage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&elevation))
                        .expect("elevation root"),
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&overview))
                        .expect("overview root"),
                },
            ],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,

        hydrology_evidence: None,
    };
    let fail = Arc::new(AtomicBool::new(false));
    let provider = ToggleProvider {
        fail: fail.clone(),
        page: Arc::new(EnvironmentPage::Elevation(elevation)),
    };
    let generator = MapChunkGenerator::new([0; 32], 0, 64)
        .with_page_provider(
            Ratio::new(1, 1).expect("compression"),
            environment,
            Arc::new(provider),
        )
        .expect("provider terrain");
    let mut overlay = ResourceOverlay::default();
    for y in 0..64 {
        for x in 0..64 {
            let tile = TileCoord::new(x, y);
            if let Some(node) = generator.object_at(tile) {
                overlay
                    .deplete(&generator, node.id, node.initial_amount)
                    .expect("resource");
            }
        }
    }
    (Terrain::Map { generator, overlay }, fail)
}

#[test]
fn resumed_provider_failure_is_a_typed_stopped_reason() {
    let config = WorldConfig::new(64, 64, Seed(0)).expect("config");
    let (mut terrain, fail) = toggle_provider_terrain();
    // Depleting the fixture's resources queried every tile. Resume the route
    // with fresh query caches so the provider outage reaches the planner.
    if let Terrain::Map { generator, .. } = &mut terrain {
        *generator = generator.fork_queries();
    }
    let mut world = GameWorld::new(config).expect("world");
    world.terrain = terrain;
    let origin = TileCoord::new(1, 1);
    // A chunk the first tick never read: cached chunks no longer ask the
    // provider, so the outage must hit terrain read for the first time.
    let destination = TileCoord::new(60, 1);
    let id = world
        .spawn_unit(
            PlayerId(0),
            WorldPosition::from_tile_center(origin).expect("origin"),
        )
        .expect("spawn");
    let index = world.lookup[&id];
    let origin_position = WorldPosition::from_tile_center(origin).expect("origin position");
    let destination_position =
        WorldPosition::from_tile_center(destination).expect("destination position");
    world.store_planner(index, Some(RoutePlanner::new(origin, destination, 4_096)));
    world.units[index].order = Some(MovementOrder {
        origin: origin_position,
        destination: destination_position,
        waypoint: origin_position,
        target_tile: destination,
        segment_length: 1,
        travelled: 1,
        speed_carry: 0,
    });
    world.units[index].state.moving = true;
    world.active_movers.insert(id);
    world.planning_budget = Some(1);

    world.advance();
    assert!(world.units[index].state.planning);
    assert_eq!(world.movement_failure(id), None);

    fail.store(true, Ordering::Relaxed);
    world.advance();
    assert_eq!(
        world.movement_failure(id),
        Some(GameWorldError::Environment(
            EnvironmentPageError::Unavailable
        ))
    );
    assert!(world.movement_order(id).is_none());
}

#[test]
fn scheduling_budget_is_part_of_canonical_state() {
    let config = WorldConfig::new(64, 64, Seed(0)).expect("config");
    let first = GameWorld::new(config).expect("first world");
    let mut second = GameWorld::new(config).expect("second world");
    assert_eq!(first.canonical_hash(), second.canonical_hash());
    second.planning_budget = Some(1);
    assert_ne!(first.canonical_hash(), second.canonical_hash());
}

mod continuity_tests;
mod detours;
mod edge_origin;
#[path = "routes/fairness.rs"]
mod fairness;
mod fights;
mod freeze;
mod guides;
mod long_run;
mod movement_ground;
mod regions;
mod saturation;
mod starvation;

#[test]
fn a_short_clear_trip_walks_straight_when_the_fast_budget_is_spent() {
    let config = WorldConfig::new(64, 64, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    world.terrain = flat_map_terrain();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).expect("tile");
    let near = world.spawn_unit(PlayerId(0), at(10, 10)).expect("spawn");
    let far = world.spawn_unit(PlayerId(0), at(10, 20)).expect("spawn");
    world.fast_route_budget = Some(0);
    assert!(world.issue_move(near, at(14, 12)).expect("short trip"));
    assert!(world.issue_move(far, at(60, 20)).expect("long trip"));
    let near = world.unit(near).expect("unit");
    assert!(!near.planning, "a short clear trip does not wait");
    assert_eq!(
        world.movement_order(near.id).expect("order").waypoint,
        at(14, 12)
    );
    assert!(
        world.unit(far).expect("unit").planning,
        "a long trip still waits"
    );
}

#[test]
fn a_mover_waiting_for_its_route_is_counted_as_waiting() {
    let config = WorldConfig::new(64, 64, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    world.terrain = flat_map_terrain();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).expect("tile");
    let far = world.spawn_unit(PlayerId(0), at(10, 20)).expect("spawn");
    world.fast_route_budget = Some(0);
    assert!(world.issue_move(far, at(60, 20)).expect("long trip"));
    assert_eq!(world.order_mover_census().iter().sum::<u64>(), 1);
    assert_eq!(world.order_waiting_census().iter().sum::<u64>(), 1);
}
