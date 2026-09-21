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
            let remaining = order.segment_length.saturating_sub(order.travelled);
            let step = remaining.min(self.config.move_speed_subunits_per_tick as u32);
            order.travelled += step;
            let arrived = order.travelled >= order.segment_length;
            let position = if arrived {
                order.waypoint
            } else {
                interpolate(order, order.travelled)
            };
            let previous_tile = self.units[index].state.previous_position.tile_floor();
            let tile = position.tile_floor();
            if !self.terrain.passable(tile, self.config)
                || (tile != previous_tile
                    && !self.terrain.crossable(previous_tile, tile, self.config))
            {
                self.units[index].state.moving = false;
                self.units[index].state.planning = false;
                self.units[index].order = None;
                changed.push(self.units[index].state);
                continue;
            }
            self.units[index].state.position = position;
            if self.units[index].bucket != ChunkCoord::from_position(position) {
                self.move_bucket(index, ChunkCoord::from_position(position));
            }
            if arrived {
                self.advance_arrived(index, id, order, &mut still_moving);
            } else {
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

    fn advance_arrived(
        &mut self,
        index: usize,
        id: aoe_core::EntityId,
        order: MovementOrder,
        still_moving: &mut Vec<aoe_core::EntityId>,
    ) {
        if order.waypoint == order.destination {
            self.units[index].state.moving = false;
            self.units[index].state.planning = false;
            self.units[index].order = None;
            return;
        }
        let next = self.units[index]
            .route
            .pop_front()
            .and_then(|tile| WorldPosition::from_tile_center(tile).ok())
            .map(SegmentAdvance::Next)
            .unwrap_or_else(|| self.next_map_segment(index, order));
        if matches!(next, SegmentAdvance::Planning) {
            self.units[index].state.moving = false;
            self.units[index].state.planning = true;
            self.units[index].order = Some(order);
            still_moving.push(id);
            return;
        }
        let SegmentAdvance::Next(next) = next else {
            self.units[index].state.moving = false;
            self.units[index].state.planning = false;
            self.units[index].order = None;
            return;
        };
        let dx = i64::from(next.x) - i64::from(order.waypoint.x);
        let dy = i64::from(next.y) - i64::from(order.waypoint.y);
        let next_order = MovementOrder {
            origin: order.waypoint,
            waypoint: next,
            segment_length: segment_length(dx, dy),
            travelled: 0,
            ..order
        };
        self.units[index].state.facing = facing_for(dx, dy, self.units[index].state.facing);
        self.units[index].state.moving = true;
        self.units[index].state.planning = false;
        self.units[index].order = Some(next_order);
        still_moving.push(id);
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
