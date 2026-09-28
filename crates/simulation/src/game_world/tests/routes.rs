use super::*;
use crate::MAX_ROUTE_WORK_PER_TICK;
use crate::game_movement::{MAX_ACTIVE_ROUTE_PLANNERS, MAX_ACTIVE_ROUTE_SEARCHES};
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
    let generator = MapChunkGenerator::new([0; 32], 0, 64)
        .with_prepared_elevation(
            Ratio::new(1, 1).expect("compression"),
            &environment,
            vec![elevation, elevation_overview],
        )
        .expect("elevation")
        .with_prepared_water(&environment, vec![water, water_overview])
        .expect("water");
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
fn resumable_planners_rotate_with_a_shared_one_expansion_budget() {
    let config = WorldConfig::new(64, 64, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    world.terrain = flat_map_terrain();
    let first = world
        .spawn_unit(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(1, 1)).expect("first"),
        )
        .expect("first unit");
    let second = world
        .spawn_unit(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(1, 3)).expect("second"),
        )
        .expect("second unit");
    let first_target = WorldPosition::from_tile_center(TileCoord::new(20, 1)).expect("target");
    let second_target = WorldPosition::from_tile_center(TileCoord::new(20, 3)).expect("target");
    world.planning_budget = 1;
    world.issue_move(first, first_target).expect("first route");
    world
        .issue_move(second, second_target)
        .expect("second route");
    world.planning_budget = 1;

    world.advance();
    let first_progress = world.units[world.lookup[&first]]
        .planner
        .as_ref()
        .expect("first planner")
        .work();
    let second_progress = world.units[world.lookup[&second]]
        .planner
        .as_ref()
        .expect("second planner")
        .work();
    assert!(first_progress > second_progress);

    world.planning_budget = 1;
    world.advance();
    let first_again = world.units[world.lookup[&first]]
        .planner
        .as_ref()
        .expect("first planner")
        .work();
    let second_again = world.units[world.lookup[&second]]
        .planner
        .as_ref()
        .expect("second planner")
        .work();
    assert!(second_again > second_progress);
    assert_eq!(first_again, first_progress);
}

#[test]
fn planner_slot_exhaustion_defers_a_new_order_without_rejecting_it() {
    let config = WorldConfig::new(64, 64, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    world.terrain = flat_map_terrain();

    let mut continuation = world
        .terrain
        .route_planner(TileCoord::new(1, 1), TileCoord::new(55, 1), 4_096)
        .expect("map planner");
    assert!(matches!(
        world
            .terrain
            .poll_route_planner(&mut continuation, 4_096, &|| false),
        Some(RoutePlannerPoll::Path(_))
    ));
    assert!(continuation.has_route_continuation());
    assert!(!continuation.requires_search_slot());

    let mut planner_units = Vec::new();
    for raw in 0..MAX_ACTIVE_ROUTE_PLANNERS {
        let tile = TileCoord::new(1 + (raw as i32 % 8), 1 + (raw as i32 / 8));
        let id = world
            .spawn_unit(
                PlayerId(0),
                WorldPosition::from_tile_center(tile).expect("origin"),
            )
            .expect("spawn");
        let index = world.lookup[&id];
        world.store_planner(
            index,
            Some(if raw < MAX_ACTIVE_ROUTE_SEARCHES {
                RoutePlanner::new(tile, TileCoord::new(tile.x + 20, tile.y), 4_096)
            } else {
                continuation.clone()
            }),
        );
        planner_units.push(id);
    }
    assert_eq!(world.active_planner_count, MAX_ACTIVE_ROUTE_PLANNERS);
    assert_eq!(world.active_route_searches, MAX_ACTIVE_ROUTE_SEARCHES);

    let queued = world
        .spawn_unit(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(10, 1)).expect("queued origin"),
        )
        .expect("queued spawn");
    let target = WorldPosition::from_tile_center(TileCoord::new(63, 1)).expect("target");
    world.planning_budget = 1;
    assert!(world.issue_move(queued, target).expect("deferred order"));
    assert!(world.movement_order(queued).is_some());
    assert!(world.unit(queued).expect("unit").planning);
    assert!(world.units[world.lookup[&queued]].planner.is_none());
    assert_eq!(world.active_planner_count, MAX_ACTIVE_ROUTE_PLANNERS);
    assert_eq!(world.active_route_searches, MAX_ACTIVE_ROUTE_SEARCHES);
    assert_eq!(world.planning_budget, 1);

    let redirected = planner_units[0];
    let redirect_target =
        WorldPosition::from_tile_center(TileCoord::new(63, 1)).expect("redirect target");
    world.planning_budget = MAX_ROUTE_WORK_PER_TICK;
    assert!(
        world
            .issue_move(redirected, redirect_target)
            .expect("redirect")
    );
    assert_eq!(world.active_planner_count, MAX_ACTIVE_ROUTE_PLANNERS);
    assert_eq!(world.active_route_searches, MAX_ACTIVE_ROUTE_SEARCHES - 1);

    world.clear_planner(world.lookup[&planner_units[1]]);
    assert_eq!(world.active_planner_count, MAX_ACTIVE_ROUTE_PLANNERS - 1);
    assert_eq!(world.active_route_searches, 0);
    world.planning_budget = MAX_ROUTE_WORK_PER_TICK;
    assert!(
        world
            .issue_move(queued, target)
            .expect("resumed deferred order")
    );
    assert!(
        world.units[world.lookup[&queued]]
            .planner
            .as_ref()
            .is_some_and(|planner| planner.has_route_continuation())
    );
    assert_eq!(world.active_planner_count, MAX_ACTIVE_ROUTE_PLANNERS);
    assert_eq!(world.active_route_searches, 0);
    assert!(world.movement_order(queued).is_some());
    for index in 0..world.units.len() {
        world.clear_planner(index);
    }
    assert_eq!(world.active_planner_count, 0);
    assert_eq!(world.active_route_searches, 0);
}

#[test]
fn resumed_provider_failure_is_a_typed_stopped_reason() {
    let config = WorldConfig::new(64, 64, Seed(0)).expect("config");
    let (terrain, fail) = toggle_provider_terrain();
    let mut world = GameWorld::new(config).expect("world");
    world.terrain = terrain;
    let origin = TileCoord::new(1, 1);
    let destination = TileCoord::new(20, 1);
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
    world.active_movers.push(id);
    world.planning_budget = 1;

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
    second.planning_budget = 1;
    assert_ne!(first.canonical_hash(), second.canonical_hash());
}
