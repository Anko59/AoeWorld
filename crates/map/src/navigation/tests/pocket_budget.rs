//! A reverse pocket flood used to run thousands of probes inside a one-work poll.
use super::*;
use std::cell::Cell;

struct Walled {
    queries: Cell<u32>,
}
impl crate::RouteTerrain for Walled {
    fn route_walkable(
        &self,
        _: &ResourceOverlay,
        tile: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        self.queries.set(self.queries.get() + 1);
        Ok(tile.x >= 0
            && tile.y >= 0
            && tile.x < 256
            && tile.y < 256
            && (tile.x != 128 || tile.y < 5))
    }
    fn route_edge_clear(
        &self,
        _: TileCoord,
        _: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        self.queries.set(self.queries.get() + 1);
        Ok(true)
    }
    fn route_step_cost(
        &self,
        from: TileCoord,
        to: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<u32, EnvironmentPageError> {
        self.queries.set(self.queries.get() + 1);
        Ok(if from.x == to.x || from.y == to.y {
            10
        } else {
            14
        })
    }
}

#[test]
fn pocket_flood_obeys_each_poll_budget_and_hashes_partial_progress() {
    let terrain = Walled {
        queries: Cell::new(0),
    };
    let overlay = ResourceOverlay::default();
    let mut stepped =
        RoutePlanner::new(TileCoord::new(126, 128), TileCoord::new(140, 128), 200_000);
    let mut batched = stepped.clone();
    for _ in 0..2_048 {
        let before = stepped.work();
        terrain.queries.set(0);
        assert_eq!(
            stepped.poll(&terrain, &overlay, 1, &|| false),
            RoutePlannerPoll::Pending
        );
        assert!(
            stepped.work() - before <= 1,
            "pocket work escaped the poll allowance"
        );
        assert!(
            terrain.queries.get() <= 20,
            "{} terrain queries in one work unit",
            terrain.queries.get()
        );
    }
    assert_eq!(stepped.work(), 2_048);
    assert_eq!(
        batched.poll(&terrain, &overlay, 2_048, &|| false),
        RoutePlannerPoll::Pending
    );
    let hash = |planner: &RoutePlanner| {
        let mut hash = blake3::Hasher::new();
        planner.update_hash(&mut hash);
        hash.finalize()
    };
    assert_eq!(
        hash(&stepped),
        hash(&batched),
        "poll boundaries cannot alter proof progress"
    );
    let before = hash(&stepped);
    stepped.poll(&terrain, &overlay, 1, &|| false);
    assert_ne!(
        before,
        hash(&stepped),
        "unfinished proof progress must be authoritative"
    );
}

#[test]
fn a_clear_far_route_keeps_walking_local_legs_without_preparing_a_global_corridor() {
    use crate::{NavigationPhase, NavigationProbe, NavigationSpan};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[derive(Debug, Default)]
    struct Probe(AtomicUsize);
    struct Span;
    impl NavigationSpan for Span {}
    impl NavigationProbe for Probe {
        fn begin(&self, phase: NavigationPhase) -> Box<dyn NavigationSpan> {
            if phase == NavigationPhase::Hierarchy {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
            Box::new(Span)
        }
    }
    let terrain = flat_terrain_with_width(4096);
    let overlay = ResourceOverlay::default();
    terrain.prepare_navigation(&overlay, &|| false).unwrap();
    let probe = Arc::new(Probe::default());
    terrain.set_navigation_probe(probe.clone());
    let mut planner = RoutePlanner::new(TileCoord::new(10, 50), TileCoord::new(3900, 50), 200_000);
    assert!(matches!(
        planner.poll(&terrain, &overlay, 8192, &|| false),
        RoutePlannerPoll::Path(_)
    ));
    planner.plan_ahead(&terrain, &overlay, 8192, &|| false);
    assert_eq!(
        probe.0.load(Ordering::Relaxed),
        0,
        "an unobstructed local leg must not prepare terrain along the whole trip"
    );
    assert!(planner.has_route_continuation());
}
