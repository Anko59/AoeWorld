//! Retained planning ahead shares the unused fair-pass work allowance.
use crate::{GameWorld, MAX_ROUTE_WORK_PER_ORDER};
use aoe_core::EntityId;
use aoe_map::RoutePlanner;

impl GameWorld {
    /// Plans the leg after the one each moving unit is walking, from what
    /// the waiting units left of the tick budget, so a walking unit does not
    /// pause between legs. A search that fails here is reported when the unit
    /// has used the segments already planned.
    pub(crate) fn plan_routes_ahead(&mut self, movers: &[EntityId], start: usize, mut budget: u32) {
        let wanting = (0..movers.len())
            .map(|turn| movers[(start + turn) % movers.len()])
            .filter_map(|id| self.lookup.get(&id).copied())
            .filter(|&index| {
                self.units[index].order.is_some()
                    && self.units[index]
                        .planner
                        .as_ref()
                        .is_some_and(RoutePlanner::wants_lookahead)
            })
            .collect::<Vec<_>>();
        for (turn, &index) in wanting.iter().enumerate() {
            if budget == 0 {
                break;
            }
            if self.planner_capacity_exhausted(index) || self.lookahead_jumps_queue(index) {
                continue;
            }
            let share = (budget / (wanting.len() - turn) as u32).clamp(1, MAX_ROUTE_WORK_PER_ORDER);
            let Some(mut planner) = self.take_planner(index) else {
                continue;
            };
            let work_before = planner.work();
            let kind = self.units[index].state.kind;
            let certified_before = planner.certified_destination();
            self.plan_world_route_ahead(&mut planner, share, &|| false, kind);
            if certified_before.is_none() && planner.certified_destination().is_some() {
                self.route_metrics
                    .sample("certified-nearest", kind, &planner);
            }
            let spent = planner.work().saturating_sub(work_before);
            self.route_metrics.lookahead_work += u64::from(spent);
            budget -= spent.min(budget);
            self.store_planner(index, Some(planner));
        }
    }

    /// A new leg search planned ahead would take a slot its player's waiting
    /// units queue for.
    fn lookahead_jumps_queue(&self, index: usize) -> bool {
        self.units[index].planner.as_ref().is_some_and(|planner| {
            !planner.requires_search_slot()
                && self.route_queued.contains(&(
                    self.units[index].state.player,
                    crate::game_world::far_route(planner.origin(), planner.destination()),
                ))
        })
    }

    fn planner_capacity_exhausted(&self, index: usize) -> bool {
        let own_planner = usize::from(self.units[index].planner.is_some());
        let own_search = usize::from(
            self.units[index]
                .planner
                .as_ref()
                .is_some_and(RoutePlanner::requires_search_slot),
        );
        let far = self.units[index].planner.as_ref().is_some_and(|planner| {
            crate::game_world::far_route(planner.origin(), planner.destination())
        });
        self.route_capacity_exhausted(self.units[index].state.player, own_planner, own_search, far)
    }
}
