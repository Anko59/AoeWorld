use crate::{
    GameUnit, GameWorld, MovementOrder,
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
                self.units[index].order = Some(order);
                still_moving.push(id);
            }
            changed.push(self.units[index].state);
        }
        self.active_movers = still_moving;
        self.tick.0 = self.tick.0.saturating_add(1);
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
        let next = self.units[index]
            .route
            .pop_front()
            .and_then(|tile| WorldPosition::from_tile_center(tile).ok())
            .or_else(|| self.next_map_segment(index, order));
        let Some(next) = next else {
            self.units[index].state.moving = false;
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
        self.units[index].order = Some(next_order);
        still_moving.push(id);
    }

    fn next_map_segment(&mut self, index: usize, order: MovementOrder) -> Option<WorldPosition> {
        match self
            .terrain
            .route_segment(order.waypoint.tile_floor(), order.target_tile)
        {
            Some(MovementOutcome::Path(path)) => {
                let mut route = VecDeque::from(path.tiles);
                if route.pop_front() != Some(order.waypoint.tile_floor()) {
                    return None;
                }
                let next = route
                    .pop_front()
                    .and_then(|tile| WorldPosition::from_tile_center(tile).ok());
                self.units[index].route = route;
                next
            }
            Some(MovementOutcome::InvalidDestination)
            | Some(MovementOutcome::Unreachable)
            | Some(MovementOutcome::BudgetExceeded) => None,
            None => (order.waypoint != order.destination)
                .then(|| next_waypoint(order.waypoint, order.target_tile, order.destination)),
        }
    }
}
