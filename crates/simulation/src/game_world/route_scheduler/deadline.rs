//! Every waiting unit gets an answer. A live 100-player match froze for hours
//! behind near searches that never settled: orders into sealed ground flooded
//! a continent at the fair share's trickle, and any building placed on the
//! flood restarted them, so the planner's own work limit was never reached.
//! They held their player's two slots while everyone behind them was refused.
//! Two bounds, both deterministic, now end such an order the way any failed
//! search ends: the order is dropped and the unit goes idle or back to its
//! job. Neither depends on the planner's work surviving a restart.
use super::{GameWorld, far_planner, far_route};
use crate::GameWorldError;
use aoe_core::EntityId;

/// Work a near route may spend while other units queue for its slot. An
/// ordinary detour commits its first leg by `ROUTE_LEG_COMMIT_WORK`; a route of
/// at most `NEAR_ROUTE_TILES` that has spent three times that is most likely
/// searching ground it cannot enter. With no queue it searches on (a forest's
/// one distant gap is worth it) until `NEAR_ROUTE_ANSWER_TICKS`.
pub(crate) const MAX_NEAR_ROUTE_WORK: u32 = 3 * aoe_map::ROUTE_LEG_COMMIT_WORK;
/// Ticks a near order may wait for its route: one minute at 20 Hz.
pub(crate) const NEAR_ROUTE_ANSWER_TICKS: u64 = 1_200;
/// Ticks a far order may wait for its next leg: three minutes at 20 Hz.
pub(crate) const FAR_ROUTE_ANSWER_TICKS: u64 = 3_600;

impl GameWorld {
    /// True when the waiting unit's route uses its player's far slots.
    pub(super) fn waits_for_far_route(&self, index: usize) -> bool {
        let unit = &self.units[index];
        unit.planner.as_ref().map_or_else(
            || {
                unit.order
                    .is_some_and(|order| far_route(order.waypoint.tile_floor(), order.target_tile))
            },
            far_planner,
        )
    }

    /// True when the waiting unit has used up its time for an answer.
    pub(super) fn route_wait_expired(&self, index: usize) -> bool {
        let limit = if self.waits_for_far_route(index) {
            FAR_ROUTE_ANSWER_TICKS
        } else {
            NEAR_ROUTE_ANSWER_TICKS
        };
        self.tick.0.saturating_sub(self.units[index].planning_since) >= limit
    }

    /// Ends the order of each unit in `ids`, whose wait has expired.
    pub(super) fn expire_route_waits(&mut self, ids: &[EntityId], changed: &mut Vec<EntityId>) {
        for &id in ids {
            let Some(&index) = self.lookup.get(&id) else {
                continue;
            };
            self.route_metrics.expired += 1;
            // Only a unit that searched has learned anything about this
            // target; one that never got a slot may ask again at once.
            let searched = self.units[index].planner.is_some();
            self.end_waiting_route(index, id, searched, changed);
        }
    }

    /// Ends the waiting near route of `id` once it is over its work bound.
    pub(super) fn end_exhausted_near_route(
        &mut self,
        id: EntityId,
        far: bool,
        changed: &mut Vec<EntityId>,
    ) {
        let Some(&index) = self.lookup.get(&id) else {
            return;
        };
        let unit = &self.units[index];
        if far || !self.waiting_for_route(index) {
            return;
        }
        let Some(planner) = unit.planner.as_ref() else {
            return;
        };
        if planner.work() < MAX_NEAR_ROUTE_WORK || planner.has_route_continuation() {
            return;
        }
        let work = planner.work();
        let (kind, sample) = (unit.state.kind, planner.clone());
        self.route_metrics.stuck += 1;
        self.route_metrics.sample("stuck", kind, &sample);
        self.route_metrics.failed(work);
        self.end_waiting_route(index, id, true, changed);
    }

    /// Gives up on a waiting unit's route: not proven unreachable, but out of
    /// its bound. With `remember` a re-sent order from the same tile is
    /// refused for a while rather than taking a slot again at the queue's back.
    pub(super) fn end_waiting_route(
        &mut self,
        index: usize,
        id: EntityId,
        remember: bool,
        changed: &mut Vec<EntityId>,
    ) {
        if remember && let Some(order) = self.units[index].order {
            self.remember_unreachable(index, order.target_tile);
        }
        self.stop_route(index, Some(GameWorldError::PathBudgetExceeded));
        if let Ok(at) = self.active_movers.binary_search(&id) {
            self.active_movers.remove(at);
        }
        changed.push(id);
    }
}
