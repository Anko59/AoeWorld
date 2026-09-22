use crate::{
    GameUnit, GameWorld, GameWorldError, MAX_ROUTE_WORK_PER_ORDER, MAX_ROUTE_WORK_PER_TICK,
    MovementOrder,
    game_path::next_waypoint,
    game_path::segment_length,
    game_world::{facing_for, interpolate},
};
use aoe_core::{ChunkCoord, WorldPosition};
use aoe_map::{MovementOutcome, RoutePlanner, RoutePlannerPoll};
use std::collections::VecDeque;

const MAX_WAYPOINTS_PER_TICK: usize = 4_096;
/// Bounds compact proven route continuations independently from search memory.
pub(crate) const MAX_ACTIVE_ROUTE_PLANNERS: usize = 64;
/// At most two planners may retain sparse B-tree search frontiers concurrently.
pub(crate) const MAX_ACTIVE_ROUTE_SEARCHES: usize = 2;

impl GameWorld {
    pub fn advance(&mut self) -> Vec<GameUnit> {
        let movers = std::mem::take(&mut self.active_movers);
        let mover_count = movers.len();
        let start = if mover_count == 0 {
            0
        } else {
            self.planning_cursor % mover_count
        };
        self.planning_cursor = if mover_count == 0 {
            0
        } else {
            (start + 1) % mover_count
        };
        let movers = movers
            .iter()
            .cycle()
            .skip(start)
            .take(mover_count)
            .copied()
            .collect::<Vec<_>>();
        let mut still_moving = Vec::with_capacity(movers.len());
        let mut changed = Vec::with_capacity(movers.len());
        for (turn, id) in movers.into_iter().enumerate() {
            let remaining_movers = mover_count.saturating_sub(turn).max(1);
            let mut planner_allowance = self
                .planning_budget
                .checked_div(u32::try_from(remaining_movers).unwrap_or(u32::MAX))
                .unwrap_or(0)
                .min(MAX_ROUTE_WORK_PER_ORDER);
            if self.planning_budget > 0 && planner_allowance == 0 {
                planner_allowance = 1;
            }
            let Some(&index) = self.lookup.get(&id) else {
                continue;
            };
            let Some(mut order) = self.units[index].order else {
                continue;
            };
            self.units[index].state.previous_position = self.units[index].state.position;
            let denominator = self.config.move_speed_subunits_per_tick_denominator;
            let speed = self.config.move_speed_subunits_per_tick as u64;
            let mut speed_budget = u128::from(order.speed_carry) + u128::from(speed);
            order.speed_carry = 0;
            let mut current_position = self.units[index].state.position;
            let mut finished = false;

            for _ in 0..MAX_WAYPOINTS_PER_TICK {
                let remaining = order.segment_length.saturating_sub(order.travelled);
                let step = u128::from(remaining).min(speed_budget / u128::from(denominator)) as u32;
                order.travelled += step;
                speed_budget -= u128::from(step) * u128::from(denominator);
                let arrived = order.travelled >= order.segment_length;
                let position = if arrived {
                    order.waypoint
                } else {
                    interpolate(order, order.travelled)
                };
                let previous_tile = current_position.tile_floor();
                let tile = position.tile_floor();
                let terrain_ok = match self
                    .terrain
                    .passable_with_cancel(tile, self.config, &|| false)
                    .and_then(|passable| {
                        if !passable {
                            Ok(false)
                        } else if tile != previous_tile {
                            self.terrain.crossable_with_cancel(
                                previous_tile,
                                tile,
                                self.config,
                                &|| false,
                            )
                        } else {
                            Ok(true)
                        }
                    }) {
                    Ok(terrain_ok) => terrain_ok,
                    Err(error) => {
                        self.clear_planner(index);
                        self.units[index].state.moving = false;
                        self.units[index].state.planning = false;
                        self.units[index].order = None;
                        self.units[index].last_movement_error =
                            Some(GameWorldError::Environment(error));
                        finished = true;
                        break;
                    }
                };
                if !terrain_ok {
                    self.clear_planner(index);
                    self.units[index].state.moving = false;
                    self.units[index].state.planning = false;
                    self.units[index].order = None;
                    self.units[index].last_movement_error = None;
                    finished = true;
                    break;
                }
                self.units[index].state.position = position;
                if self.units[index].bucket != ChunkCoord::from_position(position) {
                    self.move_bucket(index, ChunkCoord::from_position(position));
                }
                current_position = position;
                if !arrived {
                    order.speed_carry = speed_budget as u64;
                    self.units[index].state.moving = true;
                    self.units[index].state.planning = false;
                    self.units[index].order = Some(order);
                    still_moving.push(id);
                    finished = true;
                    break;
                }
                if order.waypoint == order.destination {
                    self.clear_planner(index);
                    self.units[index].state.moving = false;
                    self.units[index].state.planning = false;
                    self.units[index].order = None;
                    finished = true;
                    break;
                }
                let next = self.units[index]
                    .route
                    .pop_front()
                    .and_then(|tile| WorldPosition::from_tile_center(tile).ok())
                    .map(SegmentAdvance::Next)
                    .or_else(|| {
                        (order.waypoint != order.destination
                            && order.waypoint.tile_floor() == order.destination.tile_floor())
                        .then_some(SegmentAdvance::Next(order.destination))
                    })
                    .unwrap_or_else(|| self.next_map_segment(index, order, &mut planner_allowance));
                match next {
                    SegmentAdvance::Planning => {
                        // A deferred planner has no available path distance this
                        // tick. Keep only the fractional remainder, never a
                        // whole movement credit that could cause a later dash.
                        order.speed_carry = (speed_budget % u128::from(denominator)) as u64;
                        self.units[index].state.moving = false;
                        self.units[index].state.planning = true;
                        self.units[index].order = Some(order);
                        still_moving.push(id);
                        finished = true;
                        break;
                    }
                    SegmentAdvance::Stopped(error) => {
                        self.clear_planner(index);
                        self.units[index].state.moving = false;
                        self.units[index].state.planning = false;
                        self.units[index].order = None;
                        self.units[index].last_movement_error = error;
                        finished = true;
                        break;
                    }
                    SegmentAdvance::Next(next) => {
                        let dx = i64::from(next.x) - i64::from(order.waypoint.x);
                        let dy = i64::from(next.y) - i64::from(order.waypoint.y);
                        order = MovementOrder {
                            origin: order.waypoint,
                            waypoint: next,
                            segment_length: segment_length(dx, dy),
                            travelled: 0,
                            speed_carry: 0,
                            ..order
                        };
                        self.units[index].state.facing =
                            facing_for(dx, dy, self.units[index].state.facing);
                        if speed_budget < u128::from(denominator) {
                            order.speed_carry = speed_budget as u64;
                            self.units[index].state.moving = true;
                            self.units[index].state.planning = false;
                            self.units[index].order = Some(order);
                            still_moving.push(id);
                            finished = true;
                            break;
                        }
                    }
                }
            }
            if !finished {
                // The route is bounded even for deliberately tiny custom
                // segments and very large custom speeds. Unused whole credits
                // are discarded at the bound; only the fractional remainder
                // is carried into the next tick.
                order.speed_carry = (speed_budget % u128::from(denominator)) as u64;
                self.units[index].state.moving = true;
                self.units[index].state.planning = false;
                self.units[index].order = Some(order);
                still_moving.push(id);
            }
            changed.push(self.units[index].state);
        }
        still_moving.sort_unstable();
        self.active_movers = still_moving;
        self.tick.0 = self.tick.0.saturating_add(1);
        self.planning_budget = MAX_ROUTE_WORK_PER_TICK;
        changed.sort_by_key(|unit| unit.id);
        changed
    }

    pub(crate) fn next_map_route(
        &mut self,
        origin: aoe_core::TileCoord,
        destination: aoe_core::TileCoord,
    ) -> Option<MapRoutePlan> {
        if !self.terrain.has_map_navigation() {
            return None;
        }
        let budget = self.planning_budget.min(MAX_ROUTE_WORK_PER_ORDER);
        if let Some(outcome) = self.navigation_cache.get(origin, destination, budget) {
            self.planning_budget -= budget;
            return Some(MapRoutePlan::Outcome(outcome));
        }
        if self.active_planner_count >= MAX_ACTIVE_ROUTE_PLANNERS
            || self.active_route_searches >= MAX_ACTIVE_ROUTE_SEARCHES
        {
            return Some(MapRoutePlan::Pending(None));
        }
        let budget = self.take_planning_budget();
        let mut planner =
            self.terrain
                .route_planner(origin, destination, MAX_ROUTE_WORK_PER_ORDER)?;
        if budget == 0 {
            return Some(MapRoutePlan::Pending(Some(planner)));
        }
        match self
            .terrain
            .poll_route_planner(&mut planner, budget, &|| false)?
        {
            RoutePlannerPoll::Pending => Some(MapRoutePlan::Pending(Some(planner))),
            RoutePlannerPoll::Path(path) if planner.is_terminal() => {
                let outcome = MovementOutcome::Path(path);
                self.navigation_cache
                    .insert(origin, destination, budget, outcome.clone());
                Some(MapRoutePlan::Outcome(outcome))
            }
            RoutePlannerPoll::Path(path) => Some(MapRoutePlan::Segment(path, Some(planner))),
            RoutePlannerPoll::Complete => Some(MapRoutePlan::SearchLimit),
            RoutePlannerPoll::InvalidDestination => {
                let outcome = MovementOutcome::InvalidDestination;
                self.navigation_cache
                    .insert(origin, destination, budget, outcome.clone());
                Some(MapRoutePlan::Outcome(outcome))
            }
            RoutePlannerPoll::Unreachable => {
                let outcome = MovementOutcome::Unreachable;
                self.navigation_cache
                    .insert(origin, destination, budget, outcome.clone());
                Some(MapRoutePlan::Outcome(outcome))
            }
            RoutePlannerPoll::SearchLimit => Some(MapRoutePlan::SearchLimit),
            RoutePlannerPoll::Environment(error) => Some(MapRoutePlan::Environment(error)),
        }
    }

    fn next_map_segment(
        &mut self,
        index: usize,
        order: MovementOrder,
        allowance: &mut u32,
    ) -> SegmentAdvance {
        if !self.terrain.has_map_navigation() {
            return (order.waypoint != order.destination)
                .then(|| next_waypoint(order.waypoint, order.target_tile, order.destination))
                .map_or(SegmentAdvance::Stopped(None), SegmentAdvance::Next);
        }
        let needs_search = self.units[index]
            .planner
            .as_ref()
            .is_none_or(RoutePlanner::requires_search_slot);
        if needs_search && (*allowance == 0 || self.planner_capacity_exhausted(index)) {
            return SegmentAdvance::Planning;
        }
        let budget = if needs_search {
            self.take_planning_budget_share(allowance)
        } else {
            0
        };
        let mut planner = self.take_planner(index).unwrap_or_else(|| {
            RoutePlanner::new(
                order.waypoint.tile_floor(),
                order.target_tile,
                MAX_ROUTE_WORK_PER_ORDER,
            )
        });
        let result = self
            .terrain
            .poll_route_planner(&mut planner, budget, &|| false);
        match result {
            Some(RoutePlannerPoll::Path(path)) => {
                let mut route = VecDeque::from(path.tiles);
                if route.pop_front() != Some(order.waypoint.tile_floor()) {
                    return SegmentAdvance::Stopped(Some(GameWorldError::InvalidPosition));
                }
                let next = route
                    .pop_front()
                    .and_then(|tile| WorldPosition::from_tile_center(tile).ok());
                self.units[index].route = route;
                if planner.has_route_continuation() {
                    self.store_planner(index, Some(planner));
                }
                next.map_or(
                    SegmentAdvance::Stopped(Some(GameWorldError::InvalidPosition)),
                    SegmentAdvance::Next,
                )
            }
            Some(RoutePlannerPoll::Pending) => {
                self.store_planner(index, Some(planner));
                SegmentAdvance::Planning
            }
            Some(RoutePlannerPoll::Complete) => {
                SegmentAdvance::Stopped(Some(GameWorldError::InvalidTerrain))
            }
            Some(RoutePlannerPoll::InvalidDestination) => {
                SegmentAdvance::Stopped(Some(GameWorldError::InvalidPosition))
            }
            Some(RoutePlannerPoll::Unreachable) => {
                SegmentAdvance::Stopped(Some(GameWorldError::Unreachable))
            }
            Some(RoutePlannerPoll::SearchLimit) => {
                SegmentAdvance::Stopped(Some(GameWorldError::PathBudgetExceeded))
            }
            Some(RoutePlannerPoll::Environment(error)) => {
                SegmentAdvance::Stopped(Some(GameWorldError::Environment(error)))
            }
            None => SegmentAdvance::Stopped(Some(GameWorldError::InvalidTerrain)),
        }
    }

    fn planner_capacity_exhausted(&self, index: usize) -> bool {
        let own_planner = usize::from(self.units[index].planner.is_some());
        let own_search = usize::from(
            self.units[index]
                .planner
                .as_ref()
                .is_some_and(RoutePlanner::requires_search_slot),
        );
        self.active_planner_count.saturating_sub(own_planner) >= MAX_ACTIVE_ROUTE_PLANNERS
            || self.active_route_searches.saturating_sub(own_search) >= MAX_ACTIVE_ROUTE_SEARCHES
    }

    pub(crate) fn clear_planner(&mut self, index: usize) {
        let _ = self.take_planner(index);
    }

    pub(crate) fn store_planner(&mut self, index: usize, planner: Option<RoutePlanner>) {
        self.clear_planner(index);
        if let Some(planner) = &planner {
            self.active_planner_count = self.active_planner_count.saturating_add(1);
            if planner.requires_search_slot() {
                self.active_route_searches = self.active_route_searches.saturating_add(1);
            }
        }
        self.units[index].planner = planner;
    }

    fn take_planner(&mut self, index: usize) -> Option<RoutePlanner> {
        let planner = self.units[index].planner.take()?;
        self.active_planner_count = self.active_planner_count.saturating_sub(1);
        if planner.requires_search_slot() {
            self.active_route_searches = self.active_route_searches.saturating_sub(1);
        }
        Some(planner)
    }

    fn take_planning_budget(&mut self) -> u32 {
        let allocated = self.planning_budget.min(MAX_ROUTE_WORK_PER_ORDER);
        self.planning_budget -= allocated;
        allocated
    }

    fn take_planning_budget_share(&mut self, allowance: &mut u32) -> u32 {
        let allocated = self
            .planning_budget
            .min(*allowance)
            .min(MAX_ROUTE_WORK_PER_ORDER);
        self.planning_budget -= allocated;
        *allowance -= allocated;
        allocated
    }
}

enum SegmentAdvance {
    Next(WorldPosition),
    Planning,
    Stopped(Option<GameWorldError>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MapRoutePlan {
    Outcome(MovementOutcome),
    Segment(aoe_map::Path, Option<RoutePlanner>),
    Pending(Option<RoutePlanner>),
    SearchLimit,
    Environment(aoe_map::EnvironmentPageError),
}
