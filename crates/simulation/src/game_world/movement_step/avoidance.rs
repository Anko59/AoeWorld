//! Short-range unit obstruction complements the shared static route planner.
//! Fine collision cells bound every probe; full refinements share a tick budget.
use crate::{GameWorld, MovementOrder, UnitTask};
use aoe_core::WorldPosition;

fn delta(a: WorldPosition, b: WorldPosition) -> (i64, i64) {
    (
        i64::from(a.x) - i64::from(b.x),
        i64::from(a.y) - i64::from(b.y),
    )
}

impl GameWorld {
    pub(crate) fn local_retry(&self, id: aoe_core::EntityId) -> bool {
        self.tick.0.wrapping_add(u64::from(id.0)).is_multiple_of(8)
    }

    pub(crate) fn refine_occupied_step(
        &mut self,
        index: usize,
        order: &mut MovementOrder,
        proposed: WorldPosition,
        step: u32,
    ) -> (WorldPosition, bool) {
        let unit = self.units[index].state;
        if self.local_unit_step_clear(index, unit.position, proposed) {
            return (proposed, false);
        }
        // A foundation can invalidate the next static waypoint while the body
        // still lies in the preceding tile. Return that route to the fair
        // scheduler instead of mistaking the new wall for temporary traffic.
        if self.local_retry(unit.id)
            && (self
                .economy
                .blocked_tiles
                .contains(&order.waypoint.tile_floor())
                || !self
                    .unit_tile_passable(unit.kind, order.waypoint.tile_floor(), &|| false)
                    .unwrap_or(false))
        {
            self.clear_planner(index);
            self.units[index].route.clear();
            self.units[index].fast_route_tried = false;
            self.units[index].planning_since = self.tick.0;
            order.origin = unit.position;
            order.waypoint = unit.position;
            order.travelled = 0;
            order.segment_length = 0;
            return (unit.position, true);
        }
        self.record_unit_obstruction(index);
        let retry = self.take_local_retry(index);
        if retry
            && self.units[index]
                .obstruction
                .is_some_and(|o| o.blocked >= 8)
            && (self.units[index]
                .obstruction
                .is_some_and(|o| o.blocked >= 48)
                && self.refine_free_arrival(index, order)
                || self.refine_unit_route(index, order)
                || self.refine_free_arrival(index, order))
        {
            // Spend this tick's earned distance on the refined segment;
            // searching must not repeatedly turn a moving worker into a wait.
            order.travelled = step.min(order.segment_length);
            let mut position = super::interpolate(*order, order.travelled);
            if !self.local_unit_step_clear(index, unit.position, position) {
                position = if self.units[index].obstruction.is_some() {
                    self.avoid_unit_step(index, position)
                } else {
                    unit.position
                };
                order.origin = position;
                let (dx, dy) = delta(order.waypoint, position);
                order.segment_length = crate::game_path::segment_length(dx, dy);
                order.travelled = 0;
            }
            let (dx, dy) = delta(position, unit.position);
            self.units[index].state.facing = super::facing_for(dx, dy, unit.facing);
            return (position, false);
        }
        let (dx, dy) = delta(order.waypoint, unit.position);
        if order.waypoint != order.destination
            && dx * dx + dy * dy <= 1536 * 1536
            && self.local_position_occupied(index, order.waypoint)
        {
            // Refine past an occupied intermediate tile, retaining the static
            // route's next waypoint and certified planner continuation.
            return (unit.position, true);
        }
        // Work approaches and overlapping spawns need continuous small steps
        // to fan out. Their blocked-tick budget still bounds these attempts;
        // separated ordinary traffic gets one sidestep per retry.
        let position = self.avoid_unit_step(index, proposed);
        order.origin = position;
        let (dx, dy) = delta(order.waypoint, position);
        order.segment_length = crate::game_path::segment_length(dx, dy);
        order.travelled = 0;
        let (dx, dy) = delta(position, unit.position);
        self.units[index].state.facing = super::facing_for(dx, dy, unit.facing);
        (position, false)
    }

    pub(crate) fn local_position_occupied(&self, index: usize, at: WorldPosition) -> bool {
        self.position_occupied(index, at, false)
    }

    pub(in crate::game_world) fn position_occupied(
        &self,
        index: usize,
        at: WorldPosition,
        stationary_only: bool,
    ) -> bool {
        let _span = self
            .planning_probe
            .as_ref()
            .and_then(|p| p.movement_span("avoidance_query"));
        self.collision_occupied(index, at, stationary_only)
    }

    pub(in crate::game_world) fn clear_edge(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
        stationary_only: bool,
    ) -> bool {
        let (dx, dy) = delta(to, from);
        let parts = crate::game_path::segment_length(dx, dy).div_ceil(512) as i64;
        let mut previous = from;
        for n in 1..=parts {
            let next = WorldPosition::new(
                from.x + (dx * n / parts) as i32,
                from.y + (dy * n / parts) as i32,
            );
            if !self.clearance(index, previous, next, stationary_only) {
                return false;
            }
            previous = next;
        }
        true
    }

    /// Swept circles, including crossing traffic and stationary workers.
    /// The mover radius plus the maximum neighbour radius and half-step cover
    /// the whole segment. Existing overlaps may move apart, never tunnel.
    pub(crate) fn local_unit_step_clear(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
    ) -> bool {
        self.clearance(index, from, to, false)
    }

    fn clearance(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
        stationary_only: bool,
    ) -> bool {
        if from == to {
            return true;
        }
        let _span = self
            .planning_probe
            .as_ref()
            .and_then(|p| p.movement_span("avoidance_query"));
        // Crowded candidates need no terrain query once their sweep fails.
        self.collision_sweep(index, from, to, stationary_only, false)
            && self.body_step_clear(self.units[index].state.kind, from, to)
    }

    /// Fixed-angle right-hand sidesteps, then the other side, then a wait.
    /// No whole movement credit is retained while blocked. Terrain is checked
    /// for every candidate, so avoidance cannot cut a tree/building corner.
    pub(crate) fn avoid_unit_step(&self, index: usize, proposed: WorldPosition) -> WorldPosition {
        let _span = self
            .planning_probe
            .as_ref()
            .and_then(|p| p.movement_span("sidestep"));
        let unit = self.units[index].state;
        let from = unit.position;
        if self.config.valid_ground_position(proposed)
            && self.local_unit_step_clear(index, from, proposed)
            && self
                .unit_step_clear(unit.kind, from.tile_floor(), proposed.tile_floor())
                .unwrap_or(false)
            && self.building_step_clear(from.tile_floor(), proposed.tile_floor())
        {
            return proposed;
        }
        let (dx, dy) = delta(proposed, from);
        let budget = (dx * dx + dy * dy).isqrt();
        if budget == 0 {
            return from;
        }
        if let Some(next) = self.water_head_on_step(index, proposed) {
            return next;
        }
        let side = self.units[index].obstruction.map_or(1, |o| o.side) as i64;
        let yield_back = self.units[index]
            .obstruction
            .is_some_and(|o| o.blocked >= 32);
        let right = (-dy * side, dx * side);
        for (x, y) in [
            (dx + right.0, dy + right.1),
            right,
            (dx - right.0, dy - right.1),
            (-right.0, -right.1),
            if yield_back {
                (-dx + right.0, -dy + right.1)
            } else {
                (0, 0)
            },
            if yield_back { (-dx, -dy) } else { (0, 0) },
        ] {
            if x == 0 && y == 0 {
                continue;
            }
            let length = crate::game_path::segment_length(x, y) as i64;
            let next = WorldPosition::new(
                from.x + (x * budget / length) as i32,
                from.y + (y * budget / length) as i32,
            );
            if next != from
                && self.config.valid_ground_position(next)
                && self.local_unit_step_clear(index, from, next)
                && self
                    .unit_step_clear(unit.kind, from.tile_floor(), next.tile_floor())
                    .unwrap_or(false)
                && self.building_step_clear(from.tile_floor(), next.tile_floor())
            {
                return next;
            }
        }
        from
    }

    pub(crate) fn arrival_slot_occupied(&self, index: usize) -> bool {
        self.local_retry(self.units[index].state.id)
            && self.can_adjust_arrival(index)
            && self.units[index].order.is_some_and(|order| {
                let (dx, dy) = delta(self.units[index].state.position, order.destination);
                dx.abs().max(dy.abs()) <= 8 * 1024
                    && self
                        .destination_point_reserved(order.destination, self.units[index].state.id)
            })
    }

    pub(super) fn can_adjust_arrival(&self, index: usize) -> bool {
        let unit = self.units[index].state;
        if unit.task != UnitTask::Moving
            || self.economy.gatherers.contains_key(&unit.id)
            || self.economy.builders.contains_key(&unit.id)
            || self.economy.garrison_orders.contains_key(&unit.id)
            || self.combat.attackers.contains_key(&unit.id)
            || self.hunting_target(unit.id).is_some()
        {
            return false;
        }
        true
    }

    fn refine_free_arrival(&mut self, index: usize, order: &mut MovementOrder) -> bool {
        let unit = self.units[index].state;
        let (x, y) = delta(unit.position, order.destination);
        if !self.can_adjust_arrival(index)
            || x.abs().max(y.abs()) > 2 * 1024
            || (self.units[index].obstruction.is_none_or(|o| o.blocked < 48)
                && !self.destination_point_reserved(order.destination, unit.id)
                && !self.position_occupied(index, order.destination, true))
        {
            return false;
        }
        let Some(mut path) = self.local_free_arrival_route(index, order.destination) else {
            return false;
        };
        let Some(destination) = path.back().copied() else {
            return false;
        };
        let Some(waypoint) = path.pop_front() else {
            return false;
        };
        self.clear_planner(index);
        self.units[index].route = path;
        order.origin = unit.position;
        order.waypoint = waypoint;
        order.destination = destination;
        order.target_tile = destination.tile_floor();
        order.travelled = 0;
        let (x, y) = delta(waypoint, unit.position);
        order.segment_length = crate::game_path::segment_length(x, y);
        self.units[index].order = Some(*order);
        self.index_destination(unit.id);
        true
    }

    pub(crate) fn replace_occupied_arrival(&mut self, index: usize) {
        let _span = self
            .planning_probe
            .as_ref()
            .and_then(|p| p.movement_span("arrival_search"));
        if !self.arrival_slot_occupied(index) {
            return;
        }
        let id = self.units[index].state.id;
        let Some(order) = self.units[index].order else {
            return;
        };
        let destination = order.destination;
        let Ok(free) = self.free_arrival_destination(id, destination) else {
            return;
        };
        if free == destination {
            return;
        }
        // A local replacement must preserve the player's remaining shift queue.
        let queue = self.move_queues.remove(&id);
        let obstruction = self.units[index].obstruction;
        let _ = self.issue_task_move(id, free);
        self.units[index].obstruction = obstruction;
        if let Some(queue) = queue {
            self.move_queues.insert(id, queue);
        }
    }
}
