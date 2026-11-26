//! Fair route-search scheduling. Every tick the search budget is shared
//! between the *players* that have a unit waiting for a route, in a rotating
//! player order, and inside a player between its waiting units. There are no
//! search slots or tick budgets (#978): every waiting search is polled every
//! tick, on the workers, up to its order's own bound, so one player's flood
//! of orders, or one search that cannot finish, never holds back another
//! order.
use super::GameWorld;
use crate::{GameWorldError, MAX_ROUTE_WORK_PER_ORDER};
use aoe_core::{EntityId, PlayerId};
use aoe_map::RoutePlanner;
use std::collections::BTreeMap;

mod accounting;
mod attribution;
mod batch;
mod deadline;
mod fast;
mod fields;
mod group_end;
mod issue;
mod metrics;
mod order;
mod passes;
mod queue;
mod regions;
pub use accounting::RouteSchedulerStats;
pub use attribution::{
    ROUTE_CATEGORIES, ROUTE_DISTANCES, ROUTE_FAILURES, RouteAttribution, RouteCaller, RouteClass,
};
#[cfg(test)]
pub(crate) use deadline::NEAR_ROUTE_ANSWER_TICKS;
#[cfg(test)]
pub(crate) use fields::FIELD_CHUNK;
pub(crate) use fields::RouteGuides;
pub(crate) use metrics::{GroupWork, PlanningMeasurement};
pub use metrics::{PLANNING_SPANS, PlanningProbe, RouteMetrics};
pub use passes::{PASS_NAMES, PassCount};
pub(crate) use passes::{Pass, Passes};
pub use queue::{ROUTE_REACH, ROUTE_REQUESTERS, RouteQueueBreakdown, RouteQueueCell};

/// Work one search is given each tick it waits. Every waiting search gets
/// this slice every tick, never refused or queued, so a route of a few
/// thousand tiles is found in its first tick, while a hopeless search
/// spreads its whole order bound (12.8M) over about 200 ticks instead of
/// stalling one tick for seconds (5x replay: 95G instructions in one tick).
/// Record threshold: unit
pub(crate) const ROUTE_SLICE_WORK: u32 = 65_536;

/// Work between two looks for a region proof the planner asks for mid-leg.
/// A search still runs to its whole allowance in one tick; it is polled in
/// steps of this so a connectivity question the region layer can answer is
/// never left to the planner's own portal flood.
pub(crate) const REGION_PROOF_STEP: u32 = 16_384;
/// A route whose ends are farther apart than this (Chebyshev tiles) is far:
/// it is served in its own class, after the near ones, so a scout riding
/// across the map or an army sent to sealed-off ground never goes before a
/// villager's trip of a few dozen tiles.
pub(crate) const NEAR_ROUTE_TILES: i32 = 64;

/// True when a route from `from` to `to` is far.
pub(crate) fn far_route(from: aoe_core::TileCoord, to: aoe_core::TileCoord) -> bool {
    (from.x - to.x).abs().max((from.y - to.y).abs()) > NEAR_ROUTE_TILES
}

/// A fresh planner for `order`'s current leg.
fn order_planner(order: &super::MovementOrder) -> RoutePlanner {
    leg_planner(order.waypoint.tile_floor(), order.target_tile)
}

/// A fresh planner for a leg from `from` to `to`.
fn leg_planner(from: aoe_core::TileCoord, to: aoe_core::TileCoord) -> RoutePlanner {
    RoutePlanner::new(from, to, MAX_ROUTE_WORK_PER_ORDER)
}

fn far_planner(planner: &RoutePlanner) -> bool {
    far_route(planner.origin(), planner.destination())
}

/// True when this unit's route is an approach to something it is attacking:
/// the `attack_approach` class of the queue report (`queue::route_requester`).
///
/// A player's slots went to whoever asked first, and economy orders ask far
/// more often. In the measured 1,500-tick window of the 99-AI Mekong save a
/// player's search slots were held 79 (gather trips) and 32 (single moves) to
/// 8 (attack approaches), while 75 attack approaches waited with a median of
/// 178 ticks and up to 688 — every one of them with its one slot-free fast
/// attempt already spent (`StoredUnit::fast_route_tried`), so only a slot
/// could still answer them. A fight order is the one a player is watching,
/// so among the units of a player that still wait for a slot it is served
/// before a gather trip. This reorders a player's own queue; it adds no work
/// and raises no cap, so another player's share is untouched.
///
/// Deliberately broader than the queue report's `attack_approach` bucket: that
/// classifier tests naval and gatherer membership before `attack_target`
/// (`route_scheduler/queue.rs:50-58`), so a war galley ordered to attack is
/// reported under `ship` and a gathering villager ordered to attack under
/// `gather_trip`, while both are fights here. Preferring them is the point --
/// a galley told to attack is a fight a player is watching -- so the report
/// undercounts what this ordering prefers rather than the ordering missing a
/// fight. Narrowing this to match the bucket would drop naval attackers.
fn fight_order(unit: &super::StoredUnit) -> bool {
    unit.state.attack_target.is_some()
}

/// One player's share of the route planners alive in the world.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RouteSlots {
    pub(crate) planners: usize,
    pub(crate) searches: usize,
    /// The searches among `searches` that are far routes.
    pub(crate) far_searches: usize,
}

impl GameWorld {
    /// True while the unit's order waits for its route search: a repath
    /// now would throw that search away and start it again.
    pub(crate) fn waiting_for_route(&self, index: usize) -> bool {
        let unit = &self.units[index];
        unit.state.planning && unit.order.is_some() && unit.route.is_empty()
    }

    pub(crate) fn count_route_slots(
        &mut self,
        player: PlayerId,
        planner: &RoutePlanner,
        add: bool,
    ) {
        let slots = self.route_slots.entry(player).or_default();
        let search = usize::from(planner.requires_search_slot());
        let far = search * usize::from(far_planner(planner));
        if add {
            slots.planners += 1;
            slots.searches += search;
            slots.far_searches += far;
        } else {
            slots.planners = slots.planners.saturating_sub(1);
            slots.searches = slots.searches.saturating_sub(search);
            slots.far_searches = slots.far_searches.saturating_sub(far);
        }
        if *slots == RouteSlots::default() {
            self.route_slots.remove(&player);
        }
    }

    /// Spends `budget` on the units that wait for a route, before anything
    /// moves this tick: a route found here is walked in the same tick.
    /// Returns the budget left over.
    #[inline(never)] // a symbol of its own for the Callgrind stage counters
    pub(crate) fn serve_route_searches(&mut self, budget: u32, changed: &mut Vec<EntityId>) -> u32 {
        let probe = self.planning_probe.clone();
        let _span = probe.as_ref().and_then(|p| p.movement_span("plan_serve"));
        // Allocate between players before prioritizing a player's near work:
        // sustained near demand must never consume another player's far share.
        let mut waiting = BTreeMap::<PlayerId, Vec<EntityId>>::new();
        let mut expired = Vec::new();
        for id in self.active_movers.iter() {
            let Some(&index) = self.lookup.get(&id) else {
                continue;
            };
            let unit = &self.units[index];
            if unit.order.is_none() {
                continue;
            }
            if unit.state.planning
                && unit.route.is_empty()
                && unit
                    .planner
                    .as_deref()
                    .is_none_or(|planner| !planner.has_route_continuation())
            {
                if self.route_wait_expired(index) {
                    expired.push(id);
                } else {
                    waiting.entry(unit.state.player).or_default().push(id);
                }
            }
        }
        self.expire_route_waits(&expired, changed);
        let mut budget = budget;
        self.serve_from_fields(&mut waiting, &mut budget, changed);
        let spent = self.serve_far_groups(&waiting, changed);
        self.serve_waiting(waiting, budget.saturating_sub(spent), changed)
    }

    /// Shares `budget` between the players of `waiting` in a rotating order,
    /// and inside a player between its waiting units; returns what is left.
    fn serve_waiting(
        &mut self,
        mut waiting: BTreeMap<PlayerId, Vec<EntityId>>,
        budget: u32,
        changed: &mut Vec<EntityId>,
    ) -> u32 {
        if waiting.is_empty() {
            return budget;
        }
        // A unit that already holds a search keeps its frontier; it is served
        // before the player's units that still wait for a slot.
        for ids in waiting.values_mut() {
            // Then a fight before an economy trip, and then the longest
            // waiting first: by id, a busy player's newest units were never
            // reached and their wait grew without bound.
            ids.sort_by_key(|id| {
                let held = self
                    .lookup
                    .get(id)
                    .is_some_and(|&index| self.units[index].planner.is_some());
                let unit = &self.units[self.lookup[id]];
                (!held, !fight_order(unit), unit.planning_since, *id)
            });
        }
        // Fast work is its own budget. Offer it to near orders before a
        // far frontier can consume it, while fair work stays player-first.
        self.serve_fast_near_routes(&waiting, changed);
        for ids in waiting.values_mut() {
            ids.retain(|id| {
                self.lookup
                    .get(id)
                    .is_some_and(|&index| self.waiting_for_route(index))
            });
        }
        waiting.retain(|_, ids| !ids.is_empty());
        if waiting.is_empty() {
            return budget;
        }
        let mut players = waiting.into_iter().collect::<Vec<_>>();
        let mut budget = budget;
        // A batch holds every slot it fills until all its searches end, where
        // a serial pass freed each as its search finished. A second round
        // gives the slots the first one freed to the units it refused.
        for _ in 0..2 {
            let refused;
            (budget, refused) = self.serve_round(&players, budget, changed);
            players = refused;
            if players.is_empty() {
                break;
            }
        }
        budget
    }

    /// One batch over `players`: returns the budget left and, per player, the
    /// units still waiting that were refused a slot.
    fn serve_round(
        &mut self,
        players: &[(PlayerId, Vec<EntityId>)],
        mut budget: u32,
        changed: &mut Vec<EntityId>,
    ) -> (u32, Vec<(PlayerId, Vec<EntityId>)>) {
        let start = (self.tick.0 % players.len() as u64) as usize;
        // Every player's searches are chosen and charged first, in turn
        // order, then polled together on workers and finished in that order.
        let mut batch = (Vec::new(), Vec::new());
        let mut reserved = batch::Reserved::default();
        let mut owners = Vec::new();
        for turn in 0..players.len() {
            let (player, ids) = &players[(start + turn) % players.len()];
            let remaining_players = (players.len() - turn) as u32;
            let mut share = (budget / remaining_players).max(u32::from(budget > 0));
            budget -= share;
            let (far, near): (Vec<_>, Vec<_>) = ids
                .iter()
                .copied()
                .partition(|id| self.waits_for_far_route(self.lookup[id]));
            // Reserve inside this player's fair share. A global near/far
            // split would let another player's near flood shrink this share.
            let reserve = if far.is_empty() { 0 } else { share / 4 };
            let before = batch.1.len();
            share = self.prepare_route_class(
                (*player, false),
                &near,
                share - reserve,
                &mut reserved,
                &mut batch,
            );
            share = self.prepare_route_class(
                (*player, true),
                &far,
                share + reserve,
                &mut reserved,
                &mut batch,
            );
            owners.push((*player, before..batch.1.len()));
            budget += share;
        }
        let (searches, visits) = batch;
        let done = self.run_searches(searches, reserved, changed);
        for &(id, far, search) in &visits {
            if let Some(&(_, _, charged, spent)) = search.and_then(|at| done.get(at)) {
                // Work a search was charged but did not spend goes to plan-ahead.
                budget += charged.saturating_sub(spent);
            }
            self.end_disconnected_near_route(id, far, changed);
        }
        let refused = owners
            .into_iter()
            .filter_map(|(player, range)| {
                let ids = visits[range]
                    .iter()
                    .filter(|(id, _, search)| {
                        search.is_none()
                            && self
                                .lookup
                                .get(id)
                                .is_some_and(|&index| self.waiting_for_route(index))
                    })
                    .map(|&(id, _, _)| id)
                    .collect::<Vec<_>>();
                (!ids.is_empty()).then_some((player, ids))
            })
            .collect();
        (budget, refused)
    }

    /// Shares a class's work between the units that can spend it: those that
    /// hold a planner (first in `ids`) and as many as the free slots admit.
    /// Dividing by every waiting unit left two holders 1 or 2 work a tick
    /// behind 847 units that could not spend theirs. Each chosen search is
    /// charged its allowance; returns what is left of `share`.
    fn prepare_route_class(
        &mut self,
        (player, far): (PlayerId, bool),
        ids: &[EntityId],
        mut share: u32,
        reserved: &mut batch::Reserved,
        (searches, visits): &mut batch::Batch,
    ) -> u32 {
        for (position, &id) in ids.iter().enumerate() {
            // Even an empty fair share permits a unit's one fast attempt.
            let remaining = (ids.len() - position) as u32;
            let allowance = if share == 0 {
                0
            } else {
                (share / remaining).clamp(1, ROUTE_SLICE_WORK)
            };
            let search = self.prepare_search(player, id, allowance, reserved);
            visits.push((id, far, search.as_ref().map(|_| searches.len())));
            if let Some(search) = search {
                share -= search.fair_charge().min(share);
                searches.push(search);
            }
        }
        share
    }

    /// Ends a unit's movement order after its route search failed.
    pub(crate) fn stop_route(&mut self, index: usize, error: Option<GameWorldError>) {
        if error == Some(GameWorldError::Unreachable)
            && let Some(order) = self.units[index].order
        {
            self.remember_unreachable(index, order.target_tile);
        }
        self.clear_planner(index);
        let state = &mut self.units[index].state;
        state.moving = false;
        state.planning = false;
        self.units[index].order = None;
        self.units[index].route.clear();
        self.set_movement_error(index, error);
        let id = self.units[index].state.id;
        self.economy.gatherers.wake_walking(id);
        self.combat.healing.wake(id);
        self.refresh_spacing_order(id);
        if !self.economy.gatherers.contains_key(&id)
            && self.hunting_target(id).is_none()
            && !self.combat.attackers.contains_key(&id)
            && !self.economy.builders.contains_key(&id)
        {
            let unit = &mut self.units[index].state;
            unit.task = if unit.carried.is_some() {
                crate::UnitTask::Carrying
            } else {
                crate::UnitTask::Idle
            };
        }
    }
}
