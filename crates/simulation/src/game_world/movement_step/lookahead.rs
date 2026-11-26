//! Retained planning ahead shares the unused fair-pass work allowance.
use crate::GameWorld;
use aoe_core::EntityId;
use aoe_map::RoutePlanner;

impl GameWorld {
    /// Plans the leg after the one each moving unit is walking, from what
    /// the waiting units left of the tick budget, so a walking unit does not
    /// pause between legs. A search that fails here is reported when the unit
    /// has used the segments already planned.
    #[inline(never)] // a symbol of its own for the Callgrind stage counters
    pub(crate) fn plan_routes_ahead(&mut self, movers: &[EntityId], start: usize, mut budget: u32) {
        let wanting = (0..movers.len())
            .map(|turn| movers[(start + turn) % movers.len()])
            .filter_map(|id| self.lookup.get(&id).copied())
            .filter(|&index| {
                self.units[index].order.is_some()
                    && self.units[index]
                        .planner
                        .as_deref()
                        .is_some_and(RoutePlanner::wants_lookahead)
            })
            .collect::<Vec<_>>();
        for (turn, &index) in wanting.iter().enumerate() {
            if budget == 0 {
                break;
            }
            let share = (budget / (wanting.len() - turn) as u32)
                .clamp(1, crate::game_world::route_scheduler::ROUTE_SLICE_WORK);
            let Some(mut planner) = self.take_planner(index) else {
                continue;
            };
            let work_before = planner.work();
            let kind = self.units[index].state.kind;
            let certified_before = planner.certified_destination();
            let probe = self.planning_probe.clone();
            let span = probe.as_ref().and_then(|p| p.movement_span("plan_ahead"));
            let player = self.units[index].state.player;
            let mut left = share;
            loop {
                self.reuse_region_endpoint_proof(&mut planner, kind, player);
                let step = left.min(crate::game_world::route_scheduler::REGION_PROOF_STEP);
                let before = planner.work();
                self.plan_world_route_ahead(&mut planner, step, &|| false, kind, player);
                let spent = planner.work().saturating_sub(before);
                left -= spent.min(left);
                // Short of its step, the planner stopped by itself.
                if spent < step || left == 0 {
                    break;
                }
            }
            self.reuse_region_endpoint_proof(&mut planner, kind, player);
            drop(span);
            if certified_before.is_none() && planner.certified_destination().is_some() {
                self.route_metrics
                    .sample("certified-nearest", kind, &planner);
            }
            let spent = planner.work().saturating_sub(work_before);
            self.route_metrics.lookahead_work += u64::from(spent);
            self.demand_route_work(Some(self.units[index].state.id), 2, spent);
            let (from, to) = (planner.origin(), planner.destination());
            self.route_metrics
                .attribution
                .work(kind, from, to, 2, spent);
            budget -= spent.min(budget);
            self.store_planner(index, Some(planner));
        }
    }
}
