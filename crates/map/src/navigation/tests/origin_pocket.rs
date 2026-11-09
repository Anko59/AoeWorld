use super::*;
use crate::{EnvironmentPageError, RouteOriginPocket};
use std::cell::Cell;

struct Terrain {
    radius: i32,
    open: bool,
    probes: Cell<usize>,
}
impl RouteTerrain for Terrain {
    fn route_walkable(
        &self,
        _: &ResourceOverlay,
        _: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        self.probes.set(self.probes.get() + 1);
        Ok(true)
    }
    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        let inside = |t: TileCoord| t.x.abs().max(t.y.abs()) <= self.radius;
        // Directional edges and terrain-weighted costs exercise stale open nodes.
        Ok(self.open || (inside(from) == inside(to) && (to.x >= from.x || to.y != from.y)))
    }
    fn route_step_cost(
        &self,
        from: TileCoord,
        to: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<u32, EnvironmentPageError> {
        let base = if from.x == to.x || from.y == to.y {
            ORTHOGONAL_COST
        } else {
            DIAGONAL_COST
        };
        Ok(base * (1 + ((to.x + 20) * 13 + (to.y + 20) * 17).rem_euclid(5) as u32))
    }
}

#[test]
fn origin_pocket_charges_exact_work_without_more_terrain_queries() {
    for radius in [0, 1, 2] {
        let terrain = Terrain {
            radius,
            open: false,
            probes: Cell::new(0),
        };
        let overlay = ResourceOverlay::default();
        let origin = TileCoord::new(0, 0);
        let pocket = RouteOriginPocket::build(&terrain, &overlay, origin, &|| false)
            .unwrap()
            .unwrap();
        assert_eq!(pocket.failure_work(origin), None);
        for y in -16..=16 {
            for x in -16..=16 {
                let to = TileCoord::new(x, y);
                let before = terrain.probes.get();
                let work = pocket.failure_work(to);
                assert_eq!(
                    before,
                    terrain.probes.get(),
                    "budget projection reads no terrain"
                );
                let mut planner = RoutePlanner::new(origin, to, MAX_ROUTE_PLANNER_WORK);
                let result = planner.poll(&terrain, &overlay, 8192, &|| false);
                match work {
                    Some(work) => {
                        assert_eq!(result, RoutePlannerPoll::Unreachable);
                        assert_eq!(work, planner.work(), "radius {radius}, target {to:?}");
                        assert_eq!(pocket.explored_area(to), planner.explored_area());
                    }
                    None => assert!(matches!(result, RoutePlannerPoll::Path(_))),
                }
            }
        }
    }
}

#[test]
fn origin_pocket_never_prunes_an_open_large_or_unknown_component() {
    let terrain = |radius, open| Terrain {
        radius,
        open,
        probes: Cell::new(0),
    };
    let overlay = ResourceOverlay::default();
    let origin = TileCoord::new(0, 0);
    assert!(
        RouteOriginPocket::build(&terrain(1, true), &overlay, origin, &|| false)
            .unwrap()
            .is_none()
    );
    assert!(
        RouteOriginPocket::build(&terrain(9, false), &overlay, origin, &|| false)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        RouteOriginPocket::build(&terrain(1, false), &overlay, origin, &|| true),
        Err(EnvironmentPageError::Cancelled)
    ));
}
