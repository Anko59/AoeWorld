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
use aoe_map::RoutePlanner;

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
/// Times a formation's far wait is renewed before the group is ended. In a
/// 99-AI match every far group order of the probe was ended whole at 3,600
/// ticks while its route was still being searched or queued for a slot; a
/// group is many units and the owner's order, so it waits longer. The bound
/// stays: an order into sealed ground (the reason for the deadline) is still
/// answered, by the planner's own proof or after `1 + MAX` waits. A near
/// group's wait is renewed only while its search has spent under
/// `MAX_NEAR_ROUTE_WORK`: it waited for a slot rather than searched.
pub(crate) const MAX_GROUP_ROUTE_RENEWALS: u8 = 4;

/// Renewal and exhaustion use the same definition of a lightly searched route.
fn has_near_route_work_remaining(planner: &RoutePlanner) -> bool {
    planner.work() < MAX_NEAR_ROUTE_WORK
}

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
            let far = self.waits_for_far_route(index);
            let unit = &mut self.units[index];
            // A near carrier that searched little only queued for a slot: in
            // a 100-AI probe 35 group orders, nearly all near, ended so after
            // one minute. One that searched its bound (sealed ground) ends.
            let queued = unit
                .planner
                .as_ref()
                .is_none_or(has_near_route_work_remaining);
            if (far || queued)
                && unit.formation == Some(id)
                && unit.route_renewals < MAX_GROUP_ROUTE_RENEWALS
            {
                // The carrier keeps its order and any search it holds.
                unit.route_renewals += 1;
                unit.planning_since = self.tick.0;
                self.route_metrics.attribution.group_wait_renewals += 1;
                continue;
            }
            self.route_metrics.expired += 1;
            self.attribute_expiry(index, id);
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
        if has_near_route_work_remaining(planner) || planner.has_route_continuation() {
            return;
        }
        let work = planner.work();
        let (kind, sample) = (unit.state.kind, planner.clone());
        self.route_metrics.stuck += 1;
        self.route_metrics.sample("stuck", kind, &sample);
        self.route_metrics.failed(work);
        let (from, to) = (sample.origin(), sample.destination());
        let caller = self.units[index].route_caller;
        self.route_metrics
            .attribution
            .failed(kind, from, to, work, 3, caller);
        if self.units[index].formation == Some(id) {
            self.route_metrics.attribution.group_ends_exhausted += 1;
        }
        self.end_waiting_route(index, id, true, changed);
    }

    /// Diagnostics: who the wait bound ended (a formation's carrier ends
    /// its whole group).
    fn attribute_expiry(&mut self, index: usize, id: EntityId) {
        let unit = &self.units[index];
        let (kind, held) = (unit.state.kind, unit.planner.is_some());
        let ends = unit
            .order
            .map(|order| (order.waypoint.tile_floor(), order.target_tile));
        let members = (unit.formation == Some(id)).then(|| self.group_size(id).unwrap_or(1));
        let attribution = &mut self.route_metrics.attribution;
        if let Some((from, to)) = ends {
            attribution.expired(kind, from, to);
        }
        attribution.expired_without_planner += u64::from(!held);
        match members {
            Some(count) => {
                attribution.group_orders_expired += 1;
                attribution.group_members_expired += count as u64;
            }
            None => attribution.single_expired += 1,
        }
    }

    /// Gives up on a waiting unit's route: not proven unreachable, but out of
    /// its bound. With `remember` a re-sent order from the same tile is
    /// refused for a while rather than taking a slot again at the queue's back.
    pub(crate) fn end_waiting_route(
        &mut self,
        index: usize,
        id: EntityId,
        remember: bool,
        changed: &mut Vec<EntityId>,
    ) {
        if remember && let Some(order) = self.units[index].order {
            self.remember_unreachable(index, order.target_tile);
        }
        let group = self.end_group_of(index);
        self.stop_route(index, Some(GameWorldError::PathBudgetExceeded));
        self.fail_group_members(&group);
        if let Ok(at) = self.active_movers.binary_search(&id) {
            self.active_movers.remove(at);
        }
        changed.push(id);
    }

    /// A formation's carrier owns the group's only route, so its failure is
    /// the group's. Removes the group and returns the members besides the
    /// carrier, which the caller stops once the carrier is stopped.
    fn end_group_of(&mut self, index: usize) -> Vec<EntityId> {
        let id = self.units[index].state.id;
        // A detached member's own detour is its own: rejoin handles its end.
        let Some(leader) = self.units[index].formation.filter(|&leader| leader == id) else {
            return Vec::new();
        };
        let mut ids = self.dissolve_group(leader);
        if !ids.contains(&id) {
            ids.push(id);
        }
        for &member in &ids {
            if let Some(&at) = self.lookup.get(&member) {
                self.units[at].formation = None;
            }
        }
        // A General's squad must not stay marked as routed: its next decision
        // orders the march again (bounded by the unreachable memory).
        for &member in &ids {
            let general = self.general_controller(member).unwrap_or(member);
            if let Some(army) = self.economy.generals.armies.get_mut(&general) {
                army.routed = None;
            }
        }
        ids.retain(|&member| member != id);
        ids
    }

    fn fail_group_members(&mut self, members: &[EntityId]) {
        for &member in members {
            let Some(&at) = self.lookup.get(&member) else {
                continue;
            };
            self.stop_member(at);
            self.units[at].last_movement_error = Some(GameWorldError::PathBudgetExceeded);
        }
    }
}
