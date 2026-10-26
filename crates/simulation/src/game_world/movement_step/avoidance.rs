//! Short-range unit obstruction complements the shared static route planner.
//! Every probe uses at most four existing cells and 32 entries per cell.
use crate::{GameWorld, MovementOrder, UnitTask};
use aoe_core::WorldPosition;

const CLEARANCE: i64 = 640;

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
        self.record_unit_obstruction(index);
        let retry = self.take_local_retry(index);
        if retry && (self.refine_unit_route(index, order) || self.refine_free_arrival(index, order))
        {
            // Spend this tick's earned distance on the refined segment;
            // searching must not repeatedly turn a moving worker into a wait.
            order.travelled = step.min(order.segment_length);
            let mut position = super::interpolate(*order, order.travelled);
            if !self.local_unit_step_clear(index, unit.position, position) {
                position = if self.local_retry(unit.id) {
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
        let position = if retry
            || self.has_work_task(unit.id)
            || self.local_position_occupied(index, unit.position)
        {
            self.avoid_unit_step(index, proposed)
        } else {
            unit.position
        };
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

    pub(super) fn position_occupied(
        &self,
        index: usize,
        at: WorldPosition,
        stationary_only: bool,
    ) -> bool {
        let _span = self
            .planning_probe
            .as_ref()
            .and_then(|p| p.movement_span("avoidance_query"));
        let unit = self.units[index].state;
        self.spacing_units(at, self.tick.0.wrapping_add(u64::from(unit.id.0)), unit.id)
            .any(|other| {
                let (x, y) = delta(at, other.position);
                (!stationary_only || !self.units[self.lookup[&other.id]].state.moving)
                    && unit.kind.is_naval() == other.kind.is_naval()
                    && x * x + y * y < CLEARANCE * CLEARANCE
            })
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

    /// Swept clearance, including crossing traffic and stationary workers.
    /// Steps are at most half a tile, so the midpoint's one-tile query covers
    /// the whole swept disk (640 + 256 < 1023). Already overlapping spawns
    /// escape in id priority; separated pairs retain swept clearance.
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
        let unit = self.units[index].state;
        let (dx, dy) = delta(to, from);
        let length = dx * dx + dy * dy;
        let midpoint = WorldPosition::new(
            ((i64::from(from.x) + i64::from(to.x)) / 2) as i32,
            ((i64::from(from.y) + i64::from(to.y)) / 2) as i32,
        );
        !self
            .spacing_units(
                midpoint,
                self.tick.0.wrapping_add(u64::from(unit.id.0)),
                unit.id,
            )
            .any(|other| {
                if unit.kind.is_naval() != other.kind.is_naval()
                    || (stationary_only && self.units[self.lookup[&other.id]].state.moving)
                {
                    return false;
                }
                let (x, y) = delta(other.position, from);
                let start_distance = x * x + y * y;
                let dot = x * dx + y * dy;
                if start_distance < CLEARANCE * CLEARANCE && (dot <= 0 || unit.id < other.id) {
                    return false;
                }
                if dot <= 0 {
                    start_distance < CLEARANCE * CLEARANCE
                } else if dot >= length {
                    let (x, y) = delta(other.position, to);
                    x * x + y * y < CLEARANCE * CLEARANCE
                } else {
                    let cross = x * dy - y * dx;
                    cross * cross < CLEARANCE * CLEARANCE * length
                }
            })
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
            && self
                .unit_step_clear(unit.kind, from.tile_floor(), proposed.tile_floor())
                .unwrap_or(false)
            && self.building_step_clear(from.tile_floor(), proposed.tile_floor())
            && self.local_unit_step_clear(index, from, proposed)
        {
            return proposed;
        }
        let (dx, dy) = delta(proposed, from);
        let budget = (dx * dx + dy * dy).isqrt();
        if budget == 0 {
            return from;
        }
        for (x, y) in [
            (dx, dy),
            (dx, 0),
            (0, dy),
            (dx - dy, dy + dx),
            (dx + dy, dy - dx),
            (-dy, dx),
            (dy, -dx),
            (-dx - dy, dx - dy),
            (dy - dx, -dx - dy),
            (-dx, -dy),
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
                && self
                    .unit_step_clear(unit.kind, from.tile_floor(), next.tile_floor())
                    .unwrap_or(false)
                && self.building_step_clear(from.tile_floor(), next.tile_floor())
                && self.local_unit_step_clear(index, from, next)
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
                    && self.work_spot_busy(order.target_tile, self.units[index].state.id)
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
        if !self.can_adjust_arrival(index) || x.abs().max(y.abs()) > 8 * 1024 {
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
        self.units[index].route = path.into_iter().map(WorldPosition::tile_floor).collect();
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
