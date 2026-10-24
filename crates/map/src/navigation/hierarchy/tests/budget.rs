//! Lazy work must obey the same per-poll budget as exact tile searches.
use super::super::*;
use crate::navigation::long_routes_tests::flat_terrain_with_water_wall;
use crate::{MAX_ROUTE_PLANNER_WORK, RoutePlanner, RoutePlannerPoll};

#[test]
fn a_prepared_far_order_starts_before_exploring_its_land_mass() {
    let generator = flat_terrain_with_water_wall(16_000, 32, -1);
    let overlay = ResourceOverlay::default();
    generator.prepare_navigation(&overlay, &|| false).unwrap();
    let mut planner = RoutePlanner::new(
        TileCoord::new(100, 100),
        TileCoord::new(15_800, 100),
        MAX_ROUTE_PLANNER_WORK,
    );
    let first = planner.poll(&generator, &overlay, 16_384, &|| {
        generator.walk_cache_usage().1 > 64
    });
    assert!(matches!(first, RoutePlannerPoll::Path(_)), "{first:?}");
    assert!(generator.walk_cache_usage().1 <= 64);
    for _ in 0..200 {
        let before = generator.walk_cache_usage().1;
        planner.plan_ahead(&generator, &overlay, 1_024, &|| false);
        assert!(
            generator.walk_cache_usage().1 <= before + 12,
            "one small lookahead composed a region"
        );
    }
}

#[test]
fn exhausted_endpoint_budget_does_not_prepare_a_portal_chunk() {
    let generator = flat_terrain_with_water_wall(512, -1, -1);
    let mut search = incremental::Search::new(TileCoord::new(1, 20), TileCoord::new(500, 20), 256);
    let mut remaining = (32 * 32 * 9 + 32 * 8) * 2;
    assert_eq!(
        search.poll(&generator, &mut remaining, &|| false).unwrap(),
        None
    );
    assert_eq!(remaining, 0);
    assert!(generator.portal_cache().lock().unwrap().chunks.is_empty());
}

#[test]
fn component_refinement_is_independent_of_slices_and_cached_answers() {
    let generator = flat_terrain_with_water_wall(64, 32, -1);
    let overlay = ResourceOverlay::default();
    generator.prepare_navigation(&overlay, &|| false).unwrap();
    let origin = TileCoord::new(1, 20);
    let destination = TileCoord::new(62, 20);
    let run = |slice| {
        let mut resolution = generator
            .navigation_resolution(&overlay, origin, destination, &|| false)
            .unwrap()
            .unwrap();
        let mut work = 0;
        loop {
            let mut remaining = slice;
            let outcome = resolution
                .poll(
                    &generator,
                    &mut remaining,
                    Some((MAX_ROUTE_PLANNER_WORK, crate::MAX_ROUTE_PLANNER_NODES)),
                    &|| false,
                )
                .unwrap();
            work += slice - remaining;
            if let Some(outcome) = outcome {
                let mut hash = blake3::Hasher::new();
                resolution.update_hash(&mut hash);
                return (outcome, work, hash.finalize());
            }
        }
    };
    let cold = run(16_384);
    assert_eq!(
        cold.0,
        ResolutionOutcome::Destination(Some(TileCoord::new(31, 20)))
    );
    assert_eq!(cold, run(1));
    assert_eq!(cold, run(73));
}
