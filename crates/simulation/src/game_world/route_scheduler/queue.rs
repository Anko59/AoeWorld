//! Diagnostics only (never hashed): who holds the route queue right now.
//! Every unit with an order that waits for its route, or holds a planner,
//! classified by what asked for the route, how far it goes and how long it
//! has waited. `GET /admin/match/routes` reports it as `queue`.
use super::far_route;
use crate::{GameWorld, UnitKind};
use aoe_core::PlayerId;
use std::collections::BTreeMap;

/// What asked for the route, in the report's order.
pub const ROUTE_REQUESTERS: [&str; 6] = [
    "gather_trip",
    "single_move",
    "group_carrier",
    "member_detour",
    "ship",
    "attack_approach",
];
/// Straight distance class: near (up to 64 tiles), far, or no planner yet
/// (the destination is only known to the planner).
pub const ROUTE_REACH: [&str; 3] = ["near", "far", "unplanned"];

/// One class of the queue: units waiting for a route (with their waits in
/// ticks, sorted) and units walking while their planner still works.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RouteQueueCell {
    pub waits: Vec<u64>,
    pub active: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RouteQueueBreakdown {
    pub tick: u64,
    /// By (player, requester, reach).
    pub cells: BTreeMap<(PlayerId, usize, usize), RouteQueueCell>,
}

impl GameWorld {
    fn route_requester(&self, index: usize) -> usize {
        let unit = &self.units[index];
        let id = unit.state.id;
        if unit.state.kind.is_naval() && !self.economy.gatherers.contains_key(&id) {
            4
        } else if self.economy.gatherers.contains_key(&id)
            || matches!(unit.state.kind, UnitKind::Villager)
        {
            0
        } else if unit.state.attack_target.is_some() {
            5
        } else {
            match unit.formation {
                Some(carrier) if carrier == id => 2,
                Some(_) => 3,
                None => 1,
            }
        }
    }

    /// The queue as it stands; an off-tick read, linear in active movers.
    pub fn route_queue_breakdown(&self) -> RouteQueueBreakdown {
        let mut out = RouteQueueBreakdown {
            tick: self.tick.0,
            ..RouteQueueBreakdown::default()
        };
        for id in &self.active_movers {
            let Some(&index) = self.lookup.get(id) else {
                continue;
            };
            let unit = &self.units[index];
            if unit.order.is_none() {
                continue;
            }
            let waiting = unit.state.planning && unit.route.is_empty();
            if !waiting && unit.planner.is_none() {
                continue;
            }
            let reach = unit
                .planner
                .as_ref()
                .map_or(2, |p| usize::from(far_route(p.origin(), p.destination())));
            let cell = out
                .cells
                .entry((unit.state.player, self.route_requester(index), reach))
                .or_default();
            if waiting {
                cell.waits
                    .push(self.tick.0.saturating_sub(unit.planning_since));
            } else {
                cell.active += 1;
            }
        }
        for cell in out.cells.values_mut() {
            cell.waits.sort_unstable();
        }
        out
    }
}
