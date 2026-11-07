//! A formation's carrier owns the group's only route. When its search fails
//! after the order was issued, members used to keep steering straight at
//! their slot goals (the carrier had no order, so the group "arrived"):
//! across water and through forests, until each one gave up where it stood.
//! A search that ran out of work is load-dependent (the AI probe's groups
//! fail so under 99 AIs): when the group has marched, it now asks again from
//! where its anchor stands, as a blocked anchor step does, within the bound
//! of the carrier's wait renewals and its original wait deadline.
//! A destination proven unreachable (across a river with no bridge) no longer
//! leaves each member pressing at its slot on the far side: the group's next
//! leg goes to the reachable tile nearest the destination and it forms there.
use super::deadline::MAX_GROUP_ROUTE_RENEWALS;
use super::{GameWorld, attribution};
use crate::GameWorldError;
use crate::game_world::MovementOrder;
use aoe_core::{EntityId, TileCoord};

impl GameWorld {
    /// The search of group carrier `index` ended with `error` (reason index
    /// of `ROUTE_FAILURES`). Re-plans from the anchor when the failure is
    /// load-dependent; otherwise stops the carrier. Counted by reason.
    /// `nearest` is the reachable tile nearest the destination that the
    /// failed search proved, if any.
    pub(crate) fn group_route_failed(
        &mut self,
        index: usize,
        error: Option<GameWorldError>,
        reason: usize,
        nearest: Option<TileCoord>,
        changed: &mut Vec<EntityId>,
    ) {
        let id = self.units[index].state.id;
        let attribution = &mut self.route_metrics.attribution;
        attribution.group_orders_failed += 1;
        attribution.group_failures[reason.min(attribution::ROUTE_FAILURES.len() - 1)] += 1;
        let marched = self
            .group_anchor(id)
            .filter(|&(_, progress)| progress > 0)
            .map(|(anchor, _)| anchor);
        if error == Some(GameWorldError::PathBudgetExceeded)
            && self.units[index].route_renewals < MAX_GROUP_ROUTE_RENEWALS
            && let Some(anchor) = marched
            && let Some(order) = self.units[index].order
        {
            self.clear_planner(index);
            let unit = &mut self.units[index];
            unit.route_renewals += 1;
            unit.route.clear();
            unit.order = Some(MovementOrder {
                origin: anchor,
                waypoint: anchor,
                travelled: 0,
                segment_length: 0,
                speed_carry: 0,
                ..order
            });
            // The wait keeps its start: the deadline still answers the order.
            if !unit.state.planning {
                unit.planning_since = self.tick.0;
            }
            unit.fast_route_tried = false;
            unit.state.planning = true;
            unit.state.moving = true;
            self.route_metrics.attribution.group_replans += 1;
            if let Err(at) = self.active_movers.binary_search(&id) {
                self.active_movers.insert(at, id);
            }
            return;
        }
        // A proven failure stops the carrier as before; the group stays, and
        // each member's own recovery (a detour to its slot, the General's
        // next order) takes over. Dissolving the group here stranded
        // General squads (recovery_recording_has_a_full_army...: 53 of 101).
        let target = self.units[index].order.map(|order| order.target_tile);
        self.route_metrics.attribution.group_carriers_stopped += 1;
        self.stop_route(index, error);
        if error == Some(GameWorldError::Unreachable)
            && let Some(target) = target
            && self.stop_group_short(id, target, nearest)
        {
            self.route_metrics.attribution.group_stopped_short += 1;
        }
        if let Ok(at) = self.active_movers.binary_search(&id) {
            self.active_movers.remove(at);
        }
        changed.push(id);
    }
}
