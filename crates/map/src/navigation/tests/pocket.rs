use crate::{EnvironmentPageError, ResourceOverlay, RoutePlanner, RoutePlannerPoll, RouteTerrain};
use aoe_core::TileCoord;
use std::cell::Cell;

#[path = "detours.rs"]
mod detours;

struct Ring {
    radius: i32,
    probes: Cell<u32>,
}
impl RouteTerrain for Ring {
    fn route_walkable(
        &self,
        _: &ResourceOverlay,
        tile: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        self.probes.set(self.probes.get() + 1);
        Ok((0..256).contains(&tile.x)
            && (0..256).contains(&tile.y)
            && (tile.x - 200).abs().max((tile.y - 200).abs()) != self.radius)
    }
    fn route_edge_clear(
        &self,
        _: TileCoord,
        _: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        Ok(true)
    }
    fn route_step_cost(
        &self,
        from: TileCoord,
        to: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<u32, EnvironmentPageError> {
        Ok(if from.x == to.x || from.y == to.y {
            10
        } else {
            14
        })
    }
}

#[test]
fn destination_flood_resumes_within_each_poll_allowance() {
    for radius in [8, 18] {
        let terrain = Ring {
            radius,
            probes: Cell::new(0),
        };
        let mut planner = RoutePlanner::new(
            TileCoord::new(200, 200 - radius - 1),
            TileCoord::new(200, 200),
            1_000_000,
        );
        let overlay = ResourceOverlay::default();
        assert_eq!(
            planner.poll(&terrain, &overlay, 1024, &|| false),
            RoutePlannerPoll::Pending
        );
        for _ in 0..10_000 {
            let work = planner.work();
            terrain.probes.set(0);
            let result = planner.poll(&terrain, &overlay, 1, &|| false);
            assert!(
                planner.work() - work <= 1,
                "one-unit poll exceeded its budget"
            );
            assert!(
                terrain.probes.get() <= 4,
                "one probe expanded the whole pocket"
            );
            if result != RoutePlannerPoll::Pending {
                assert_eq!(result, RoutePlannerPoll::Unreachable);
                assert_eq!(radius, 8);
                break;
            }
        }
    }
}

#[test]
fn suspended_proof_invalidation_includes_rejected_neighbours() {
    let terrain = Ring {
        radius: 2,
        probes: Cell::new(0),
    };
    let mut planner = RoutePlanner::new(
        TileCoord::new(200, 197),
        TileCoord::new(200, 200),
        1_000_000,
    );
    let overlay = ResourceOverlay::default();
    let entrance = aoe_core::TileRect::from_xywh(198, 200, 1, 1);
    assert_eq!(
        planner.poll(&terrain, &overlay, 1024, &|| false),
        RoutePlannerPoll::Pending
    );
    assert!(!planner.opening_invalidates_proof(entrance));
    assert_eq!(
        planner.poll(&terrain, &overlay, 16, &|| false),
        RoutePlannerPoll::Pending
    );
    // Blocked neighbours are absent from the retained frontier. Testing
    // only `touches` would miss an opening on this rejected ground.
    assert!(!planner.touches(entrance));
    let before = planner.clone();
    terrain.probes.set(0);
    assert!(planner.opening_invalidates_proof(entrance));
    assert!(!planner.opening_invalidates_proof(aoe_core::TileRect::from_xywh(10, 10, 1, 1)));
    assert_eq!(terrain.probes.get(), 0, "invalidation does no route work");
    assert_eq!(planner, before, "checking invalidation preserves progress");
    assert_eq!(
        planner.poll(&terrain, &overlay, 1000, &|| false),
        RoutePlannerPoll::Unreachable
    );
    assert!(!planner.opening_invalidates_proof(entrance));
}
