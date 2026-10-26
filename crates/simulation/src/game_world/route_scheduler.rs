//! Fair route-search scheduling. Every tick the search budget is shared
//! between the *players* that have a unit waiting for a route, in a rotating
//! player order, and inside a player between its waiting units. Search memory
//! (retained frontiers) is bounded per player, so one player's flood of
//! orders, or one search that cannot finish, delays only that player's own
//! orders and never another player's.
use super::GameWorld;
use crate::game_movement::{MAX_ACTIVE_ROUTE_PLANNERS, MAX_ACTIVE_ROUTE_SEARCHES};
use crate::{GameWorldError, MAX_ROUTE_WORK_PER_ORDER};
use aoe_core::{EntityId, PlayerId};
use aoe_map::{MovementOutcome, RoutePlanner, RoutePlannerPoll};
use std::collections::{BTreeMap, VecDeque};

mod accounting;
mod fast;
mod issue;
mod metrics;
mod order;
pub use accounting::RouteSchedulerStats;
pub(crate) use metrics::PlanningMeasurement;
pub use metrics::{PlanningProbe, RouteMetrics};

/// Retained search frontiers one player may hold at once.
pub(crate) const MAX_ROUTE_SEARCHES_PER_PLAYER: usize = 2;
/// Compact route continuations one player may hold at once.
pub(crate) const MAX_ROUTE_PLANNERS_PER_PLAYER: usize = 64;
/// A route whose ends are farther apart than this (Chebyshev tiles) searches
/// in its player's far slots. A scout riding across the map, or an army sent
/// to sealed-off ground, searches for minutes; in a 99-AI match such searches
/// held both of a player's slots while its villagers' trips of a few dozen
/// tiles waited behind them. Near and far searches now have slots of their
/// own, `MAX_ROUTE_SEARCHES_PER_PLAYER` each.
pub(crate) const NEAR_ROUTE_TILES: i32 = 64;

/// True when a route from `from` to `to` uses its player's far slots.
pub(crate) fn far_route(from: aoe_core::TileCoord, to: aoe_core::TileCoord) -> bool {
    (from.x - to.x).abs().max((from.y - to.y).abs()) > NEAR_ROUTE_TILES
}

fn far_planner(planner: &RoutePlanner) -> bool {
    far_route(planner.origin(), planner.destination())
}

/// One player's share of the route planners alive in the world.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RouteSlots {
    pub(crate) planners: usize,
    pub(crate) searches: usize,
    /// The searches among `searches` that are far routes.
    pub(crate) far_searches: usize,
}

impl RouteSlots {
    /// Searches held in the slots of the class `far` names.
    pub(crate) fn class_searches(&self, far: bool) -> usize {
        if far {
            self.far_searches
        } else {
            self.searches - self.far_searches
        }
    }
}

impl GameWorld {
    /// True when `player` may not start one more planner of the given kind
    /// (a search of a far route when `far`): its own share is used up, or the
    /// world-wide memory bound is reached. `own_*` discount the planner the
    /// unit already holds (of the same class).
    pub(crate) fn route_capacity_exhausted(
        &self,
        player: PlayerId,
        own_planner: usize,
        own_search: usize,
        far: bool,
    ) -> bool {
        let slots = self.route_slots.get(&player).copied().unwrap_or_default();
        slots.planners.saturating_sub(own_planner) >= MAX_ROUTE_PLANNERS_PER_PLAYER
            || slots.class_searches(far).saturating_sub(own_search) >= MAX_ROUTE_SEARCHES_PER_PLAYER
            || self.active_planner_count.saturating_sub(own_planner) >= MAX_ACTIVE_ROUTE_PLANNERS
            || self.active_route_searches.saturating_sub(own_search) >= MAX_ACTIVE_ROUTE_SEARCHES
    }

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
    pub(crate) fn serve_route_searches(&mut self, budget: u32, changed: &mut Vec<EntityId>) -> u32 {
        // Allocate between players before prioritizing a player's near work:
        // sustained near demand must never consume another player's far share.
        let mut waiting = BTreeMap::<PlayerId, Vec<EntityId>>::new();
        for &id in &self.active_movers {
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
                    .as_ref()
                    .is_none_or(|planner| !planner.has_route_continuation())
            {
                waiting.entry(unit.state.player).or_default().push(id);
            }
        }
        self.route_queued.clear();
        self.serve_waiting(waiting, budget, changed)
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
            // Then the longest-waiting first: by id, a busy player's newest
            // units were never reached and their wait grew without bound.
            ids.sort_by_key(|id| {
                let held = self
                    .lookup
                    .get(id)
                    .is_some_and(|&index| self.units[index].planner.is_some());
                let unit = &self.units[self.lookup[id]];
                (!held, unit.planning_since, *id)
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
        let players = waiting.into_iter().collect::<Vec<_>>();
        let start = (self.tick.0 % players.len() as u64) as usize;
        let mut budget = budget;
        for turn in 0..players.len() {
            let (player, ids) = &players[(start + turn) % players.len()];
            let remaining_players = (players.len() - turn) as u32;
            let mut share = (budget / remaining_players).max(u32::from(budget > 0));
            budget -= share;
            let (far, near): (Vec<_>, Vec<_>) = ids.iter().copied().partition(|id| {
                let unit = &self.units[self.lookup[id]];
                unit.planner.as_ref().map_or_else(
                    || {
                        unit.order.is_some_and(|order| {
                            far_route(order.waypoint.tile_floor(), order.target_tile)
                        })
                    },
                    far_planner,
                )
            });
            // Reserve inside this player's fair share. A global near/far
            // split would let another player's near flood shrink this share.
            let reserve = if far.is_empty() { 0 } else { share / 4 };
            share = self.serve_route_class(*player, &near, share - reserve, changed);
            share = self.serve_route_class(*player, &far, share + reserve, changed);
            budget += share;
        }
        budget
    }

    fn serve_route_class(
        &mut self,
        player: PlayerId,
        ids: &[EntityId],
        mut share: u32,
        changed: &mut Vec<EntityId>,
    ) -> u32 {
        for (position, &id) in ids.iter().enumerate() {
            // Even an empty fair share permits a unit's one fast attempt.
            let remaining = (ids.len() - position) as u32;
            let allowance = if share == 0 {
                0
            } else {
                (share / remaining).clamp(1, MAX_ROUTE_WORK_PER_ORDER)
            };
            let spent = self.search_waiting_route(player, id, allowance, changed);
            share -= spent.min(share);
        }
        share
    }

    /// One waiting unit's search with at most `allowance` work of the fair
    /// budget; returns the fair work spent. A unit that holds no search slot
    /// first gets its fast attempt (once per leg, from the fast budget); only
    /// a search that attempt cannot settle takes a slot, and with no slot free
    /// it keeps no frontier.
    fn search_waiting_route(
        &mut self,
        player: PlayerId,
        id: EntityId,
        allowance: u32,
        changed: &mut Vec<EntityId>,
    ) -> u32 {
        let Some(&index) = self.lookup.get(&id) else {
            return 0;
        };
        let Some(order) = self.units[index].order else {
            return 0;
        };
        let kind = self.units[index].state.kind;
        let held = self.units[index].planner.as_ref();
        let holds_planner = usize::from(held.is_some());
        let holds_search = held.is_some_and(RoutePlanner::requires_search_slot);
        let far = held.map_or_else(
            || far_route(order.waypoint.tile_floor(), order.target_tile),
            far_planner,
        );
        // A retained zero/partial-work planner still owes its full fast
        // attempt after an exhausted issue-time budget; retention is no proof
        // that the attempt ran. This matters with resumable reverse proofs.
        let fast = !self.units[index].fast_route_tried && self.fast_route_allowance(player) > 0;
        if !fast && allowance == 0 {
            return 0;
        }
        if !holds_search && !fast && self.route_capacity_exhausted(player, holds_planner, 0, far) {
            self.route_queued.insert((player, far));
            self.route_metrics.slot_refusals += 1;
            return 0;
        }
        let mut planner = self.take_planner(index).unwrap_or_else(|| {
            RoutePlanner::new(
                order.waypoint.tile_floor(),
                order.target_tile,
                MAX_ROUTE_WORK_PER_ORDER,
            )
        });
        let fresh = planner.work() == 0;
        let work_before = planner.work();
        let (result, spent) = if fast {
            let (result, full) = self.poll_fast_route(&mut planner, kind, player);
            self.units[index].fast_route_tried = full;
            (result, 0)
        } else {
            let result = self.poll_world_route_planner(&mut planner, allowance, &|| false, kind);
            let spent = planner.work().saturating_sub(work_before);
            if work_before == 0 && spent > 0 {
                self.route_metrics.searches_started += 1;
            }
            self.route_metrics.fair_work += u64::from(spent);
            (result, spent)
        };
        let error = match result {
            RoutePlannerPoll::Path(path) => {
                self.route_metrics.found(planner.work());
                if planner.certified_destination().is_some() {
                    self.route_metrics
                        .sample("certified-nearest", kind, &planner);
                }
                if planner.work() >= 65_536 {
                    self.route_metrics.sample("slow", kind, &planner);
                }
                if fast && fresh && planner.is_terminal() {
                    self.cache_route(&planner, kind, &MovementOutcome::Path(path.clone()));
                }
                let mut route = VecDeque::from(path.tiles);
                if route.pop_front() == Some(order.waypoint.tile_floor()) {
                    self.units[index].route = route;
                    self.units[index].fast_route_tried = false;
                    // A certified nearest endpoint still owes the order its
                    // terminal Unreachable result after this final segment.
                    if !planner.is_terminal()
                        || planner.has_route_continuation()
                        || planner.certified_destination().is_some()
                    {
                        self.store_planner(index, Some(planner));
                    }
                    return spent;
                }
                Some(GameWorldError::InvalidPosition)
            }
            RoutePlannerPoll::Pending => {
                if fast && self.route_capacity_exhausted(player, 0, 0, far) {
                    // No slot: keep only the compact continuation, if any.
                    self.route_metrics.yielded += 1;
                    planner.abandon_leg_search();
                    if !planner.requires_search_slot() {
                        self.store_planner(index, Some(planner));
                    }
                    return spent;
                }
                self.store_planner(index, Some(planner));
                return spent;
            }
            RoutePlannerPoll::Complete => Some(GameWorldError::InvalidTerrain),
            RoutePlannerPoll::InvalidDestination => Some(GameWorldError::InvalidPosition),
            RoutePlannerPoll::Unreachable => Some(GameWorldError::Unreachable),
            RoutePlannerPoll::SearchLimit => Some(GameWorldError::PathBudgetExceeded),
            RoutePlannerPoll::Environment(error) => Some(GameWorldError::Environment(error)),
        };
        if fast && fresh && error == Some(GameWorldError::Unreachable) {
            self.cache_route(&planner, kind, &MovementOutcome::Unreachable);
        }
        self.route_metrics.failed(planner.work());
        self.stop_route(index, error);
        if let Ok(at) = self.active_movers.binary_search(&id) {
            self.active_movers.remove(at);
        }
        changed.push(id);
        spent
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
        self.units[index].last_movement_error = error;
        let id = self.units[index].state.id;
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
