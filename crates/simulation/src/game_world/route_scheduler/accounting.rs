//! What the route scheduler holds, recounted from the units themselves: the
//! stored per-player and world counters must always equal this recount, and
//! the server logs it so a stalled player shows up in a live match.
use super::{MAX_ROUTE_PLANNERS_PER_PLAYER, MAX_ROUTE_SEARCHES_PER_PLAYER, RouteSlots};
use crate::GameWorld;
use crate::game_movement::{MAX_ACTIVE_ROUTE_PLANNERS, MAX_ACTIVE_ROUTE_SEARCHES};
use aoe_core::PlayerId;
use std::collections::BTreeMap;

/// A snapshot of route-scheduler occupancy.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RouteSchedulerStats {
    /// Route planners alive (compact continuations and searches).
    pub planners: usize,
    /// Planners holding a heavy search slot.
    pub searches: usize,
    /// Units with an order that wait for a route.
    pub waiting_units: usize,
    /// Waiting units that hold no planner yet (they wait for a slot).
    pub waiting_without_planner: usize,
    /// Players with at least one waiting unit.
    pub waiting_players: usize,
    /// Most waiting units of one player, and that player.
    pub most_waiting: usize,
    pub most_waiting_player: Option<PlayerId>,
    /// Ticks the longest-waiting unit has waited for its route.
    pub oldest_wait_ticks: u64,
    pub busiest_player_planners: usize,
    pub busiest_player_searches: usize,
    pub players_at_planner_cap: usize,
    pub players_at_search_cap: usize,
    /// True when the world-wide planner or search bound is reached.
    pub world_capacity_exhausted: bool,
    /// Planners held by units that are not active movers: nothing serves them.
    pub orphan_planners: usize,
    /// Debug description of the longest-waiting unit, for the server log.
    pub oldest_waiting: Option<String>,
    /// Cumulative route demand and throughput counters.
    pub metrics: super::RouteMetrics,
}

impl GameWorld {
    /// Per-player slots and world totals recounted from the stored planners.
    pub(crate) fn recount_route_slots(&self) -> (BTreeMap<PlayerId, RouteSlots>, usize, usize) {
        let mut slots = BTreeMap::<PlayerId, RouteSlots>::new();
        let (mut planners, mut searches) = (0, 0);
        for unit in &self.units {
            let Some(planner) = &unit.planner else {
                continue;
            };
            let search = usize::from(planner.requires_search_slot());
            let entry = slots.entry(unit.state.player).or_default();
            entry.planners += 1;
            entry.searches += search;
            entry.far_searches +=
                search * usize::from(super::far_route(planner.origin(), planner.destination()));
            planners += 1;
            searches += search;
        }
        (slots, planners, searches)
    }

    /// `None` when the stored counters equal the recount, else what differs.
    pub fn route_accounting_error(&self) -> Option<String> {
        let (slots, planners, searches) = self.recount_route_slots();
        (slots != self.route_slots
            || planners != self.active_planner_count
            || searches != self.active_route_searches)
            .then(|| {
                format!(
                    "route accounting drifted: stored planners {} searches {} slots {:?}; \
                     recounted planners {planners} searches {searches} slots {slots:?}",
                    self.active_planner_count, self.active_route_searches, self.route_slots
                )
            })
    }

    fn describe_waiting(&self, index: usize) -> String {
        let unit = &self.units[index];
        let slots = self
            .route_slots
            .get(&unit.state.player)
            .copied()
            .unwrap_or_default();
        let planner = unit.planner.as_ref().map(|planner| {
            format!(
                "work {} leg_work {} search {} continuation {} terminal {}",
                planner.work(),
                planner.leg_work(),
                planner.requires_search_slot(),
                planner.has_route_continuation(),
                planner.is_terminal()
            )
        });
        format!(
            "unit {} player {} {:?} task {:?} at {:?} order {:?} planner {planner:?} \
             player slots {slots:?} last error {:?}",
            unit.state.id.0,
            unit.state.player.0,
            unit.state.kind,
            unit.state.task,
            unit.state.position.tile_floor(),
            unit.order
                .map(|order| (order.waypoint.tile_floor(), order.target_tile)),
            unit.last_movement_error
        )
    }

    pub fn route_scheduler_stats(&self) -> RouteSchedulerStats {
        let mut waiting = BTreeMap::<PlayerId, usize>::new();
        let mut metrics = self.route_metrics.clone();
        self.group_work.snapshot(&mut metrics);
        let mut stats = RouteSchedulerStats {
            planners: self.active_planner_count,
            searches: self.active_route_searches,
            metrics,
            world_capacity_exhausted: self.active_planner_count >= MAX_ACTIVE_ROUTE_PLANNERS
                || self.active_route_searches >= MAX_ACTIVE_ROUTE_SEARCHES,
            ..RouteSchedulerStats::default()
        };
        for id in &self.active_movers {
            let Some(&index) = self.lookup.get(id) else {
                continue;
            };
            let unit = &self.units[index];
            if !unit.state.planning || unit.order.is_none() || !unit.route.is_empty() {
                continue;
            }
            stats.waiting_units += 1;
            stats.waiting_without_planner += usize::from(unit.planner.is_none());
            let waited = self.tick.0.saturating_sub(unit.planning_since);
            if waited >= stats.oldest_wait_ticks {
                stats.oldest_wait_ticks = waited;
                stats.oldest_waiting = Some(self.describe_waiting(index));
            }
            *waiting.entry(unit.state.player).or_default() += 1;
        }
        stats.orphan_planners = self
            .units
            .iter()
            .filter(|unit| {
                unit.planner.is_some() && self.active_movers.binary_search(&unit.state.id).is_err()
            })
            .count();
        stats.waiting_players = waiting.len();
        if let Some((player, count)) = waiting.iter().max_by_key(|(_, count)| **count) {
            stats.most_waiting = *count;
            stats.most_waiting_player = Some(*player);
        }
        for slots in self.route_slots.values() {
            stats.busiest_player_planners = stats.busiest_player_planners.max(slots.planners);
            stats.busiest_player_searches = stats.busiest_player_searches.max(slots.searches);
            stats.players_at_planner_cap +=
                usize::from(slots.planners >= MAX_ROUTE_PLANNERS_PER_PLAYER);
            stats.players_at_search_cap += usize::from(
                slots.class_searches(false) >= MAX_ROUTE_SEARCHES_PER_PLAYER
                    || slots.class_searches(true) >= MAX_ROUTE_SEARCHES_PER_PLAYER,
            );
        }
        stats
    }
}
