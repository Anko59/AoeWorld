//! The fast attempt. In a 100-player match nearly every order is a villager's
//! trip of a few tiles, proven with a few hundred units of work, yet such
//! orders used to wait behind the few searches that hold their player's two
//! search slots. Now every order is first polled with `FAST_ROUTE_WORK` from
//! a per-tick world budget of its own (`MAX_FAST_ROUTE_WORK_PER_TICK`): a
//! route, or a proof that there is none, is answered at once with no slot and
//! no queue. Only an order its fast attempt cannot settle needs a slot, and
//! one that gets none keeps no frontier.
use crate::game_movement::{
    FAST_ROUTE_WORK, MAX_FAST_ROUTE_WORK_PER_PLAYER, MAX_FAST_ROUTE_WORK_PER_TICK,
};
use crate::{GameWorld, MAX_ROUTE_WORK_PER_ORDER, UnitKind};
use aoe_core::{PlayerId, TileCoord};
use aoe_map::{MovementOutcome, RoutePlanner, RoutePlannerPoll};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MapRoutePlan {
    Outcome(MovementOutcome),
    Segment(aoe_map::Path, Option<RoutePlanner>),
    /// The order waits for its route: the planner when it holds a slot, and
    /// whether its fast attempt was made in full.
    Pending(Option<RoutePlanner>, bool),
    SearchLimit,
    Environment(aoe_map::EnvironmentPageError),
}

impl GameWorld {
    /// Both route budgets are refilled at the end of every tick.
    pub(crate) fn refill_route_budgets(&mut self) {
        self.planning_budget = crate::MAX_ROUTE_WORK_PER_TICK;
        self.local_refinement_left = 4;
        self.combat.position_searches = super::super::combat::MAX_POSITION_SEARCHES_PER_TICK;
        *self
            .navigation_preparation_budget
            .get_mut()
            .unwrap_or_else(|error| error.into_inner()) = crate::MAX_NAV_PREPARATIONS_PER_TICK;
        self.fast_route_budget = MAX_FAST_ROUTE_WORK_PER_TICK;
        self.fast_route_spent.clear();
    }

    /// Work `player`'s next fast attempt may spend this tick.
    pub(crate) fn fast_route_allowance(&self, player: PlayerId) -> u32 {
        let spent = self.fast_route_spent.get(&player).copied().unwrap_or(0);
        self.fast_route_budget
            .min(FAST_ROUTE_WORK)
            .min(MAX_FAST_ROUTE_WORK_PER_PLAYER.saturating_sub(spent))
    }

    /// Polls `planner` with `player`'s fast allowance and charges the work
    /// spent. Returns the poll and whether the allowance was a full one.
    pub(crate) fn poll_fast_route(
        &mut self,
        planner: &mut RoutePlanner,
        kind: UnitKind,
        player: PlayerId,
    ) -> (RoutePlannerPoll, bool) {
        let allowance = self.fast_route_allowance(player);
        if allowance == 0 {
            return (RoutePlannerPoll::Pending, false);
        }
        let before = planner.work();
        let result = self.poll_world_route_planner(planner, allowance, &|| false, kind);
        let spent = planner.work().saturating_sub(before);
        if before == 0 && spent > 0 {
            self.route_metrics.searches_started += 1;
        }
        if planner.certified_destination().is_some() {
            self.route_metrics
                .sample("certified-nearest", kind, planner);
        }
        self.charge_fast_route(player, spent);
        self.route_metrics.issue_work += u64::from(spent);
        (result, allowance == FAST_ROUTE_WORK)
    }

    fn charge_fast_route(&mut self, player: PlayerId, work: u32) {
        self.fast_route_budget = self.fast_route_budget.saturating_sub(work);
        let spent = self.fast_route_spent.entry(player).or_default();
        *spent = spent.saturating_add(work);
    }

    /// Records what a fresh planner proved within one poll, keyed by its
    /// origin and destination; a hit then answers the same order exactly as
    /// a fresh search would (see `NavigationCache`).
    pub(crate) fn cache_route(
        &mut self,
        planner: &RoutePlanner,
        kind: UnitKind,
        outcome: &MovementOutcome,
    ) {
        if !kind.is_naval() && planner.certified_destination().is_none() {
            let _span = self
                .planning_probe
                .as_ref()
                .and_then(|probe| probe.cache_span());
            self.navigation_cache.insert(
                planner.origin(),
                planner.destination(),
                planner.work(),
                planner.explored_area(),
                outcome.clone(),
            );
        }
    }

    pub(crate) fn next_map_route(
        &mut self,
        origin: TileCoord,
        destination: TileCoord,
        kind: UnitKind,
        player: PlayerId,
    ) -> Option<MapRoutePlan> {
        if !self.has_route_navigation() {
            return None;
        }
        self.route_metrics.orders += 1;
        let allowance = self.fast_route_allowance(player);
        if !kind.is_naval()
            && let Some((outcome, work)) = self.navigation_cache.get(origin, destination, allowance)
        {
            self.charge_fast_route(player, work);
            self.route_metrics.cache_hits += 1;
            self.route_metrics.solved_at_issue += 1;
            return Some(MapRoutePlan::Outcome(outcome));
        }
        let mut planner = RoutePlanner::new(origin, destination, MAX_ROUTE_WORK_PER_ORDER);
        let (result, full) = self.poll_fast_route(&mut planner, kind, player);
        let work = planner.work();
        let settled = |world: &mut Self, outcome: MovementOutcome| {
            world.cache_route(&planner, kind, &outcome);
            Some(MapRoutePlan::Outcome(outcome))
        };
        match result {
            RoutePlannerPoll::Pending => {
                self.route_metrics.deferred(origin, destination);
                let far = super::far_route(origin, destination);
                if self.route_capacity_exhausted(player, 0, 0, far)
                    || self.route_queued.contains(&(player, far))
                {
                    self.route_metrics.deferred_without_slot += 1;
                    Some(MapRoutePlan::Pending(None, full))
                } else {
                    Some(MapRoutePlan::Pending(Some(planner), full))
                }
            }
            RoutePlannerPoll::Path(path) => {
                self.route_metrics.solved_at_issue += 1;
                self.route_metrics.fast_path += 1;
                self.route_metrics.found(work);
                if planner.is_terminal() && planner.certified_destination().is_none() {
                    settled(self, MovementOutcome::Path(path))
                } else {
                    Some(MapRoutePlan::Segment(path, Some(planner)))
                }
            }
            RoutePlannerPoll::Complete | RoutePlannerPoll::SearchLimit => {
                self.route_metrics.failed(work);
                Some(MapRoutePlan::SearchLimit)
            }
            RoutePlannerPoll::InvalidDestination => {
                self.route_metrics.failed(work);
                settled(self, MovementOutcome::InvalidDestination)
            }
            RoutePlannerPoll::Unreachable => {
                self.route_metrics.failed(work);
                settled(self, MovementOutcome::Unreachable)
            }
            RoutePlannerPoll::Environment(error) => Some(MapRoutePlan::Environment(error)),
        }
    }
}
