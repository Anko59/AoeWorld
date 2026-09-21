use crate::{
    GameUnit, GameWorld, MAX_ROUTE_EXPANSIONS_PER_ORDER, MAX_ROUTE_EXPANSIONS_PER_TICK,
    MovementOrder,
    game_path::next_waypoint,
    game_path::segment_length,
    game_world::{facing_for, interpolate},
};
use aoe_core::{ChunkCoord, WorldPosition};
use aoe_map::MovementOutcome;
use std::collections::VecDeque;

const MAX_WAYPOINTS_PER_TICK: usize = 4_096;

impl GameWorld {
    pub fn advance(&mut self) -> Vec<GameUnit> {
        let movers = std::mem::take(&mut self.active_movers);
        let mut still_moving = Vec::with_capacity(movers.len());
        let mut changed = Vec::with_capacity(movers.len());
        for id in movers {
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
                if !self.terrain.passable(tile, self.config)
                    || (tile != previous_tile
                        && !self.terrain.crossable(previous_tile, tile, self.config))
                {
                    self.units[index].state.moving = false;
                    self.units[index].state.planning = false;
                    self.units[index].order = None;
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
                    .unwrap_or_else(|| self.next_map_segment(index, order));
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
                    SegmentAdvance::Stopped => {
                        self.units[index].state.moving = false;
                        self.units[index].state.planning = false;
                        self.units[index].order = None;
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
        self.active_movers = still_moving;
        self.tick.0 = self.tick.0.saturating_add(1);
        self.planning_budget = MAX_ROUTE_EXPANSIONS_PER_TICK;
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
        let budget = self.take_planning_budget();
        if budget == 0 {
            return Some(MapRoutePlan::Deferred);
        }
        self.cached_map_segment(origin, destination, budget)
            .map(MapRoutePlan::Outcome)
    }

    fn next_map_segment(&mut self, index: usize, order: MovementOrder) -> SegmentAdvance {
        if !self.terrain.has_map_navigation() {
            return (order.waypoint != order.destination)
                .then(|| next_waypoint(order.waypoint, order.target_tile, order.destination))
                .map_or(SegmentAdvance::Stopped, SegmentAdvance::Next);
        }
        let budget = self.take_planning_budget();
        if budget == 0 {
            return SegmentAdvance::Planning;
        }
        match self.cached_map_segment(order.waypoint.tile_floor(), order.target_tile, budget) {
            Some(MovementOutcome::Path(path)) => {
                let mut route = VecDeque::from(path.tiles);
                if route.pop_front() != Some(order.waypoint.tile_floor()) {
                    return SegmentAdvance::Stopped;
                }
                let next = route
                    .pop_front()
                    .and_then(|tile| WorldPosition::from_tile_center(tile).ok());
                self.units[index].route = route;
                next.map_or(SegmentAdvance::Stopped, SegmentAdvance::Next)
            }
            Some(MovementOutcome::InvalidDestination) | Some(MovementOutcome::Unreachable) => {
                SegmentAdvance::Stopped
            }
            // The planner ran and exhausted its bounded search. This is a
            // terminal result for this order; only a zero shared budget is a
            // transient deferral handled above.
            Some(MovementOutcome::BudgetExceeded) => SegmentAdvance::Stopped,
            None => SegmentAdvance::Stopped,
        }
    }

    fn cached_map_segment(
        &mut self,
        origin: aoe_core::TileCoord,
        destination: aoe_core::TileCoord,
        budget: u32,
    ) -> Option<MovementOutcome> {
        if let Some(outcome) = self.navigation_cache.get(origin, destination, budget) {
            return Some(outcome);
        }
        let outcome = self
            .terrain
            .route_segment_with_limit(origin, destination, budget)?;
        self.navigation_cache
            .insert(origin, destination, budget, outcome.clone());
        Some(outcome)
    }

    fn take_planning_budget(&mut self) -> u32 {
        let allocated = self.planning_budget.min(MAX_ROUTE_EXPANSIONS_PER_ORDER);
        self.planning_budget -= allocated;
        allocated
    }
}

enum SegmentAdvance {
    Next(WorldPosition),
    Planning,
    Stopped,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MapRoutePlan {
    Outcome(MovementOutcome),
    Deferred,
}
