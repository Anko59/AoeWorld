//! Fixed-node detours around stationary jobs in narrow building lanes.
use super::*;

impl GameWorld {
    pub(super) fn refine_work_detour(&mut self, index: usize, order: &mut MovementOrder) -> bool {
        let reserve =
            u32::from(self.units[index].formation.is_none() && !self.formations.is_empty()) * 2;
        if self.local_refinement_left <= reserve {
            return false;
        }
        self.local_refinement_left -= 1;
        // Half-tile edges cover four tiles in each direction with the same
        // 289 lattice nodes and exact swept-body checks as ordinary refinement.
        let Some(mut path) = self.local_search(index, order.destination, true, STEP * 2) else {
            return false;
        };
        let from = self.units[index].state.position;
        let Some(&end) = path.back() else {
            return false;
        };
        if distance(end, order.destination).saturating_add(64) >= distance(from, order.destination)
        {
            return false;
        }
        let Some(waypoint) = path.pop_front() else {
            return false;
        };
        self.clear_planner(index);
        self.units[index].route = path;
        self.units[index].fast_route_tried = false;
        self.units[index].planning_since = self.tick.0;
        order.origin = from;
        order.waypoint = waypoint;
        order.travelled = 0;
        order.segment_length = distance(from, waypoint);
        true
    }
}
