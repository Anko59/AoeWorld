//! The fast attempt. In a 100-player match nearly every order is a villager's
//! trip of a few tiles, proven with a few hundred units of work. Every order
//! is first polled with `FAST_ROUTE_WORK`: a route, or a proof that there is
//! none, is answered at once. An order its fast attempt cannot settle keeps
//! its frontier and searches on, a slice every tick (no slots since #978).
use crate::game_movement::FAST_ROUTE_WORK;
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
    /// Refilled at the end of every tick, scaled with the tick rate. Route
    /// work has no budget (#978); a test's hold lasts one tick.
    pub(crate) fn refill_route_budgets(&mut self) {
        let config = self.config;
        self.planning_budget.take();
        self.local_refinement_left = config.scaled_budget(4);
        *self
            .navigation_preparation_budget
            .get_mut()
            .unwrap_or_else(|error| error.into_inner()) =
            config.scaled_budget(crate::MAX_NAV_PREPARATIONS_PER_TICK);
        self.fast_route_budget.take();
    }

    /// Work the next fast attempt may spend.
    pub(crate) fn fast_route_allowance(&self) -> u32 {
        self.fast_route_budget
            .map_or(FAST_ROUTE_WORK, |left| left.min(FAST_ROUTE_WORK))
    }

    /// Polls `planner` with `player`'s fast allowance and charges the work
    /// spent. Returns the poll and whether the allowance was a full one.
    pub(crate) fn poll_fast_route(
        &mut self,
        planner: &mut RoutePlanner,
        kind: UnitKind,
        player: PlayerId,
    ) -> (RoutePlannerPoll, bool) {
        let allowance = self.fast_route_allowance();
        if allowance == 0 {
            return (RoutePlannerPoll::Pending, false);
        }
        let before = planner.work();
        let probe = self.planning_probe.clone();
        let span = probe.as_ref().and_then(|p| p.movement_span("plan_fast"));
        self.reuse_region_endpoint_proof(planner, kind, player);
        let result = self.poll_world_route_planner(planner, allowance, &|| false, kind, player);
        self.reuse_region_endpoint_proof(planner, kind, player);
        drop(span);
        let spent = planner.work().saturating_sub(before);
        if before == 0 && spent > 0 {
            self.route_metrics.searches_started += 1;
        }
        if planner.certified_destination().is_some() {
            self.route_metrics
                .sample("certified-nearest", kind, planner);
        }
        self.charge_fast_route(spent);
        self.route_metrics.issue_work += u64::from(spent);
        self.demand_route_work(None, 0, spent);
        let (from, to) = (planner.origin(), planner.destination());
        self.route_metrics
            .attribution
            .work(kind, from, to, 0, spent);
        (result, allowance == FAST_ROUTE_WORK)
    }

    pub(super) fn charge_fast_route(&mut self, work: u32) {
        if let Some(left) = &mut self.fast_route_budget {
            *left = left.saturating_sub(work);
        }
    }

    /// Records what a fresh planner proved within one poll, keyed by its
    /// origin and destination; a hit then answers the same order exactly as
    /// a fresh search would (see `NavigationCache`).
    pub(crate) fn cache_route(
        &mut self,
        planner: &RoutePlanner,
        kind: UnitKind,
        player: PlayerId,
        outcome: &MovementOutcome,
    ) {
        if planner.certified_destination().is_none() {
            let _span = self
                .planning_probe
                .as_ref()
                .and_then(|probe| probe.cache_span());
            self.navigation_cache.insert(
                (kind.is_naval(), player),
                planner.origin(),
                planner.destination(),
                planner.work(),
                planner.explored_area(),
                outcome.clone(),
            );
        }
    }

    /// A near contact order (a task's work or build spot; reaching a tile
    /// beside it is no use) is asked of the region layer before any search:
    /// one the regions prove disconnected is refused without one (#978).
    pub(crate) fn contact_route_disconnected(
        &self,
        planner: &mut RoutePlanner,
        kind: UnitKind,
        player: PlayerId,
    ) -> bool {
        !kind.is_naval()
            && !super::far_route(planner.origin(), planner.destination())
            && aoe_map::LandBucket::for_radius(kind.collision_radius())
                .is_some_and(|bucket| self.near_route_disconnected(player, bucket, planner))
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
        let caller = self.route_metrics.caller;
        let issuer = self.current_route_issuer();
        self.route_metrics
            .attribution
            .order(kind, origin, destination, caller);
        let mut planner = RoutePlanner::new(origin, destination, MAX_ROUTE_WORK_PER_ORDER);
        // An area order keeps its search: refinement may serve it from the
        // nearest reachable tile.
        if self.routing_contact && self.contact_route_disconnected(&mut planner, kind, player) {
            self.route_metrics.stuck += 1;
            self.route_metrics
                .attribution
                .failed(kind, origin, destination, 0, 0, caller, issuer);
            return Some(MapRoutePlan::Outcome(MovementOutcome::Unreachable));
        }
        let allowance = self.fast_route_allowance();
        if let Some((outcome, work)) =
            self.navigation_cache
                .get((kind.is_naval(), player), origin, destination, allowance)
        {
            self.charge_fast_route(work);
            self.route_metrics.cache_hits += 1;
            self.route_metrics.solved_at_issue += 1;
            if !matches!(outcome, MovementOutcome::Path(_)) {
                self.route_metrics.attribution.cached_failure(caller);
            }
            return Some(MapRoutePlan::Outcome(outcome));
        }
        self.order_diagnostics.demand[self.order_diagnostics.issuer as usize].route_searches += 1;
        let (result, full) = self.poll_fast_route(&mut planner, kind, player);
        let work = planner.work();
        let reason = super::attribution::failure_reason(&result);
        let fail = |world: &mut Self| {
            world.route_metrics.failed(work);
            world.route_metrics.attribution.failed(
                kind,
                origin,
                destination,
                work,
                reason,
                caller,
                issuer,
            );
        };
        let settled = |world: &mut Self, outcome: MovementOutcome| {
            world.cache_route(&planner, kind, player, &outcome);
            Some(MapRoutePlan::Outcome(outcome))
        };
        match result {
            RoutePlannerPoll::Pending => {
                self.route_metrics.deferred(origin, destination);
                Some(MapRoutePlan::Pending(Some(planner), full))
            }
            RoutePlannerPoll::Path(path) => {
                self.route_metrics.solved_at_issue += 1;
                self.route_metrics.fast_path += 1;
                self.route_metrics.found(work);
                self.route_metrics
                    .attribution
                    .found(kind, origin, destination);
                if planner.is_terminal() && planner.certified_destination().is_none() {
                    settled(self, MovementOutcome::Path(path))
                } else {
                    Some(MapRoutePlan::Segment(path, Some(planner)))
                }
            }
            RoutePlannerPoll::Complete | RoutePlannerPoll::SearchLimit => {
                fail(self);
                Some(MapRoutePlan::SearchLimit)
            }
            RoutePlannerPoll::InvalidDestination => {
                fail(self);
                settled(self, MovementOutcome::InvalidDestination)
            }
            RoutePlannerPoll::Unreachable => {
                fail(self);
                settled(self, MovementOutcome::Unreachable)
            }
            RoutePlannerPoll::Environment(error) => Some(MapRoutePlan::Environment(error)),
        }
    }
}
