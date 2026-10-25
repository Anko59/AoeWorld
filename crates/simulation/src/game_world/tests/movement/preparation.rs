//! The shared logical preparation limit must hold even with warm physical caches.
use super::*;
use aoe_map::{NavigationPhase, NavigationProbe, NavigationSpan, RoutePlanner};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn preparation_is_bounded_across_planners_and_independent_of_cache_residency() {
    #[derive(Debug, Default)]
    struct Probe(AtomicUsize);
    struct Span;
    impl NavigationSpan for Span {}
    impl NavigationProbe for Probe {
        fn begin(&self, phase: NavigationPhase) -> Box<dyn NavigationSpan> {
            if phase == NavigationPhase::Terrain {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
            Box::new(Span)
        }
    }
    let mut world = wide_world();
    world.terrain = prepared_terrain(vec![0; 64 * 64], vec![BARREN; 64 * 64]);
    let probe = Arc::new(Probe::default());
    if let Terrain::Map { generator, overlay } = &mut world.terrain {
        overlay
            .add_footprint(TileRect::from_xywh(128, 0, 1, WIDTH))
            .unwrap();
        generator.set_navigation_probe(probe.clone());
    }
    world.prepare_navigation(&|| false).unwrap();
    *world.navigation_preparation_budget.lock().unwrap() = 0;
    let mut planners = (0..3)
        .map(|_| RoutePlanner::new(TileCoord::new(126, 128), TileCoord::new(512, 128), 65_536))
        .collect::<Vec<_>>();
    for planner in &mut planners {
        // Prepay more than one poll's credit while execution is forbidden;
        // completed credits must still share the next tick's execution cap.
        world.poll_world_route_planner(planner, 32_768, &|| false, UnitKind::Scout);
        assert_eq!(
            world.poll_world_route_planner(planner, 32_768, &|| false, UnitKind::Scout),
            aoe_map::RoutePlannerPoll::Pending
        );
    }
    assert_eq!(
        probe.0.load(Ordering::Relaxed),
        0,
        "a spent preparation allowance still composed terrain"
    );
    let mut warm = planners.clone();
    *world.navigation_preparation_budget.lock().unwrap() = 2;
    for planner in &mut planners {
        world.poll_world_route_planner(planner, 32_768, &|| false, UnitKind::Scout);
    }
    assert_eq!(*world.navigation_preparation_budget.lock().unwrap(), 0);
    assert!(
        probe.0.load(Ordering::Relaxed) <= 2,
        "one tick composed more than two chunks"
    );
    probe.0.store(0, Ordering::Relaxed);
    *world.navigation_preparation_budget.lock().unwrap() = 2;
    for planner in &mut warm {
        world.poll_world_route_planner(planner, 32_768, &|| false, UnitKind::Scout);
    }
    assert_eq!(*world.navigation_preparation_budget.lock().unwrap(), 0);
    assert_eq!(
        probe.0.load(Ordering::Relaxed),
        0,
        "the second traversal should use resident chunks"
    );
    for (cold, warm) in planners.iter().zip(&warm) {
        let mut a = blake3::Hasher::new();
        cold.update_hash(&mut a);
        let mut b = blake3::Hasher::new();
        warm.update_hash(&mut b);
        assert_eq!(
            a.finalize(),
            b.finalize(),
            "cache warming changed authoritative progress"
        );
    }
}
