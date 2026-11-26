//! A tick's waiting searches: chosen and charged in order, polled on workers
//! against the same world, then finished in order. While the batch polls, no
//! poll prepares a cold chunk; one that wants to is polled again alone after
//! the others, so every result is a function of the world as the batch began
//! and of the batch order, never of the thread count.
use super::{attribution, far_planner};
use crate::game_movement::FAST_ROUTE_WORK;
use crate::game_world::{MovementOrder, parallel::par_each_mut};
use crate::{GameWorld, GameWorldError, UnitKind};
use aoe_core::{EntityId, PlayerId};
use aoe_map::{MovementOutcome, RoutePlanner, RoutePlannerPoll};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// One prepared search: its planner, taken out of the unit, and the work it
/// may spend, already charged.
pub(super) struct Search {
    index: usize,
    pub(super) id: EntityId,
    player: PlayerId,
    kind: UnitKind,
    order: MovementOrder,
    pub(super) far: bool,
    /// A fast attempt's allowance and whether it was a full one.
    fast: Option<(u32, bool)>,
    /// Fair work this search may spend.
    pub(super) allowance: u32,
    planner: RoutePlanner,
    fresh: bool,
    work_before: u32,
    denied: AtomicBool,
    result: Option<RoutePlannerPoll>,
}

impl Search {
    /// Fair work charged for this search: none for a fast attempt.
    pub(super) fn fair_charge(&self) -> u32 {
        if self.fast.is_some() {
            0
        } else {
            self.allowance
        }
    }
}

/// A batch's searches, and every unit it visited in order (with the index of
/// its search, if it got one).
pub(super) type Batch = (Vec<Search>, Vec<(EntityId, bool, Option<usize>)>);

/// The planners a batch's prepared searches hold until it finishes, as
/// (player, far class): every planner is taken first, so each search counts
/// what it would store and the planner and search counts stay exact.
#[derive(Default)]
pub(super) struct Reserved(Vec<(PlayerId, bool)>);

impl GameWorld {
    /// One waiting unit's search with at most `allowance` work of the fair
    /// budget; returns the fair work spent. A unit that holds no search slot
    /// first gets its fast attempt (once per leg, from the fast budget); only
    /// a search that attempt cannot settle takes a slot, and with no slot free
    /// it keeps no frontier.
    pub(super) fn search_waiting_route(
        &mut self,
        player: PlayerId,
        id: EntityId,
        allowance: u32,
        changed: &mut Vec<EntityId>,
    ) -> u32 {
        let mut reserved = Reserved::default();
        let Some(mut search) = self.prepare_search(player, id, allowance, &mut reserved) else {
            return 0;
        };
        let probe = self.planning_probe.clone();
        let name = if search.fast.is_some() {
            "plan_fast"
        } else {
            "plan_fair"
        };
        let span = probe.as_ref().and_then(|p| p.movement_span(name));
        {
            let _measurement = self.measure_planning();
            let budget = &self.navigation_preparation_budget;
            search.result = Some(self.poll_search(&mut search, budget));
        }
        drop(span);
        self.release(reserved);
        self.finish_search(search, changed)
    }

    /// Polls `searches` on workers, then finishes them in order; returns the
    /// fair work each spent.
    pub(super) fn run_searches(
        &mut self,
        mut searches: Vec<Search>,
        reserved: Reserved,
        changed: &mut Vec<EntityId>,
    ) -> Vec<(EntityId, bool, u32, u32)> {
        // (id, far, fair work charged, fair work spent) per search, in order.
        let none = Mutex::new(0);
        {
            let world = &*self;
            par_each_mut(&mut searches, |search| {
                search.result = Some(world.poll_search(search, &none));
            });
        }
        for search in &mut searches {
            // A denied chunk only asks for more allowance: a poll that still
            // ended (a path found locally) keeps its result.
            if search.denied.swap(false, Ordering::Relaxed)
                && matches!(search.result, Some(RoutePlannerPoll::Pending))
            {
                // It wanted a cold chunk: alone now, from the tick's budget.
                let budget = &self.navigation_preparation_budget;
                search.result = Some(self.poll_search(search, budget));
            }
        }
        self.release(reserved);
        searches
            .into_iter()
            .map(|search| {
                let (id, far, charged) = (search.id, search.far, search.fair_charge());
                (id, far, charged, self.finish_search(search, changed))
            })
            .collect()
    }

    /// Counts (or uncounts) a planner holding a search slot of `far`.
    fn hold_search_slot(&mut self, player: PlayerId, far: bool, add: bool) {
        let slots = self.route_slots.entry(player).or_default();
        if add {
            slots.planners += 1;
            slots.searches += 1;
            slots.far_searches += usize::from(far);
            self.active_planner_count += 1;
            self.active_route_searches += 1;
        } else {
            slots.planners = slots.planners.saturating_sub(1);
            slots.searches = slots.searches.saturating_sub(1);
            slots.far_searches = slots.far_searches.saturating_sub(usize::from(far));
            self.active_planner_count = self.active_planner_count.saturating_sub(1);
            self.active_route_searches = self.active_route_searches.saturating_sub(1);
        }
        if *slots == super::RouteSlots::default() {
            self.route_slots.remove(&player);
        }
    }

    /// Gives back what a batch's searches held, before they finish.
    fn release(&mut self, reserved: Reserved) {
        for (player, far) in reserved.0 {
            self.hold_search_slot(player, far, false);
        }
    }

    /// Chooses and charges one unit's search; `None` when it gets none.
    pub(super) fn prepare_search(
        &mut self,
        player: PlayerId,
        id: EntityId,
        allowance: u32,
        reserved: &mut Reserved,
    ) -> Option<Search> {
        let &index = self.lookup.get(&id)?;
        let order = self.units[index].order?;
        let kind = self.units[index].state.kind;
        let far = self.waits_for_far_route(index);
        // A retained zero/partial-work planner still owes its full fast
        // attempt after an exhausted issue-time budget; retention is no proof
        // that the attempt ran. This matters with resumable reverse proofs.
        let fast = !self.units[index].fast_route_tried
            && self.fast_route_allowance(player) >= FAST_ROUTE_WORK;
        if !fast && allowance == 0 {
            return None;
        }
        let mut planner = self
            .take_planner(index)
            .unwrap_or_else(|| super::order_planner(&order));
        let fresh = planner.work() == 0;
        let class = far_planner(&planner);
        self.hold_search_slot(player, class, true);
        reserved.0.push((player, class));
        self.reuse_region_endpoint_proof(&mut planner, kind, player);
        let fast = fast.then(|| {
            self.charge_fast_route(player, FAST_ROUTE_WORK);
            (FAST_ROUTE_WORK, true)
        });
        Some(Search {
            index,
            id,
            player,
            kind,
            order,
            far,
            fast,
            allowance,
            work_before: planner.work(),
            planner,
            fresh,
            denied: AtomicBool::new(false),
            result: None,
        })
    }

    /// The poll alone: reads the world, writes only the search.
    fn poll_search(&self, search: &mut Search, preparations: &Mutex<u32>) -> RoutePlannerPoll {
        let spent = search.planner.work().saturating_sub(search.work_before);
        let budget = match search.fast {
            Some((0, _)) => return RoutePlannerPoll::Pending,
            Some((fast, _)) => fast,
            None => search.allowance,
        }
        .saturating_sub(spent);
        // In steps, answering region proofs between them (`REGION_PROOF_STEP`).
        let mut left = budget;
        loop {
            let step = left.min(super::REGION_PROOF_STEP);
            let before = search.planner.work();
            let poll = self.poll_route_planner_limited(
                &mut search.planner,
                step,
                &|| false,
                search.kind,
                search.player,
                preparations,
                Some(&search.denied),
            );
            let spent = search.planner.work().saturating_sub(before);
            left -= spent.min(left);
            if !matches!(poll, RoutePlannerPoll::Pending)
                || spent < step
                || left == 0
                || search.denied.load(Ordering::Relaxed)
            {
                return poll;
            }
            self.reuse_region_endpoint_proof(&mut search.planner, search.kind, search.player);
        }
    }

    /// Charges what the search spent and acts on its result; returns the fair
    /// work spent.
    fn finish_search(&mut self, search: Search, changed: &mut Vec<EntityId>) -> u32 {
        let Search {
            index,
            id,
            player,
            kind,
            order,
            fast,
            mut planner,
            fresh,
            work_before,
            result,
            ..
        } = search;
        let result = result.unwrap_or(RoutePlannerPoll::Pending);
        let spent = planner.work().saturating_sub(work_before);
        if work_before == 0 && spent > 0 {
            self.route_metrics.searches_started += 1;
        }
        let (from, to) = (planner.origin(), planner.destination());
        let spent = if let Some((allowance, full)) = fast {
            // Charged in full when prepared: give back what it did not use.
            self.fast_route_budget += allowance.saturating_sub(spent);
            if let Some(used) = self.fast_route_spent.get_mut(&player) {
                *used = used.saturating_sub(allowance.saturating_sub(spent));
            }
            if planner.certified_destination().is_some() {
                self.route_metrics
                    .sample("certified-nearest", kind, &planner);
            }
            self.route_metrics.issue_work += u64::from(spent);
            self.demand_route_work(Some(id), 0, spent);
            self.route_metrics
                .attribution
                .work(kind, from, to, 0, spent);
            self.units[index].fast_route_tried = full;
            0
        } else {
            self.route_metrics.fair_work += u64::from(spent);
            self.demand_route_work(Some(id), 1, spent);
            self.route_metrics
                .attribution
                .work(kind, from, to, 1, spent);
            spent
        };
        let fast = fast.is_some();
        self.reuse_region_endpoint_proof(&mut planner, kind, player);
        let reason = attribution::failure_reason(&result);
        let error = match result {
            RoutePlannerPoll::Path(path) => {
                self.route_metrics.found(planner.work());
                self.route_metrics.attribution.found(kind, from, to);
                if planner.certified_destination().is_some() {
                    self.route_metrics
                        .sample("certified-nearest", kind, &planner);
                }
                if planner.work() >= 65_536 {
                    self.route_metrics.sample("slow", kind, &planner);
                }
                if fast && fresh && planner.is_terminal() {
                    self.cache_route(&planner, kind, player, &MovementOutcome::Path(path.clone()));
                }
                let mut route = VecDeque::from(path.tiles);
                if route.pop_front() == Some(order.waypoint.tile_floor()) {
                    self.units[index].route = route
                        .into_iter()
                        .filter_map(|t| aoe_core::WorldPosition::from_tile_center(t).ok())
                        .collect();
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
                self.store_planner(index, Some(planner));
                return spent;
            }
            RoutePlannerPoll::Complete => Some(GameWorldError::InvalidTerrain),
            RoutePlannerPoll::InvalidDestination => {
                // The origin may be a tile no route starts from (the water
                // beside a bridge deck, #1040): the next round searches from
                // the passable tile the body overlaps instead.
                let origin = self.route_origin(kind, order.waypoint);
                if origin != order.waypoint {
                    if let Some(current) = self.units[index].order.as_mut() {
                        current.origin = origin;
                        current.waypoint = origin;
                    }
                    return spent;
                }
                Some(GameWorldError::InvalidPosition)
            }
            RoutePlannerPoll::Unreachable => Some(GameWorldError::Unreachable),
            RoutePlannerPoll::SearchLimit => Some(GameWorldError::PathBudgetExceeded),
            RoutePlannerPoll::Environment(error) => Some(GameWorldError::Environment(error)),
        };
        if fast && fresh && error == Some(GameWorldError::Unreachable) {
            self.cache_route(&planner, kind, player, &MovementOutcome::Unreachable);
        }
        self.route_metrics.failed(planner.work());
        let caller = self.units[index].route_caller;
        self.route_metrics
            .attribution
            .failed(kind, from, to, planner.work(), reason, caller);
        if self.units[index].formation == Some(id) {
            let nearest = planner.nearest_reachable();
            self.group_route_failed(index, error, reason, nearest, changed);
            return spent;
        }
        self.stop_route(index, error);
        self.active_movers.remove(id);
        changed.push(id);
        spent
    }
}
