//! Read-only route probes for the real-match harness (`scale-match
//! --march-probe`): the route an order would take on the static terrain and
//! the current resource overlay, outside the game's budgets.
use super::GameWorld;
use crate::{Terrain, UnitKind};
use aoe_map::{RoutePlanner, RoutePlannerPoll};

impl GameWorld {
    /// Polls `planner` with `budget` work. Spends no tick budget, preparation
    /// allowance or counter, so a probe never changes the simulation.
    pub fn probe_route_poll(
        &self,
        planner: &mut RoutePlanner,
        budget: u32,
        kind: UnitKind,
    ) -> RoutePlannerPoll {
        match &self.terrain {
            Terrain::Map { generator, overlay } if !kind.is_naval() => {
                planner.poll(generator, overlay, budget, &|| false)
            }
            _ => self.poll_world_route_planner(planner, budget, &|| false, kind),
        }
    }
}
