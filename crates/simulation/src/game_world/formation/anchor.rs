use super::*;
use crate::game_movement::SegmentAdvance;
use crate::game_path::segment_length;

impl GameWorld {
    /// Scheduler storage belongs to the first member; only this shared point
    /// consumes its route. No physical member is moved by this loop.
    pub(crate) fn advance_formation_anchor(
        &mut self,
        index: usize,
        still_moving: &mut Vec<EntityId>,
    ) -> bool {
        let leader = self.units[index].state.id;
        let Some(mut group) = self.formations.remove(&leader) else {
            return false;
        };
        let previous_denominator = group.speed.1;
        group.speed = self.group_speed(&group);
        if let Some(mut order) = self.units[index].order {
            let (speed, denominator) = group.speed;
            if denominator != previous_denominator {
                // Carry is fractional distance. A change of slowest member
                // must not reinterpret the old denominator as whole steps.
                order.speed_carry = (u128::from(order.speed_carry) * u128::from(denominator)
                    / u128::from(previous_denominator)) as u64;
            }
            let denom = denominator * 4;
            // Only a rear straggler slows the anchor; lateral assembly and
            // members already ahead must not throttle the whole march.
            let pace = speed
                * if self.formation_cohesive(&group) {
                    4
                } else {
                    3
                };
            group.pace = (pace, denom);
            self.refresh_group_path(index, &mut group, order);
            let mut credit = u128::from(order.speed_carry) + u128::from(pace);
            order.speed_carry = 0;
            for _ in 0..32 {
                let step = u128::from(order.segment_length.saturating_sub(order.travelled))
                    .min(credit / u128::from(denom)) as u32;
                order.travelled += step;
                credit -= u128::from(step) * u128::from(denom);
                let arrived = order.travelled >= order.segment_length;
                let next = if arrived {
                    order.waypoint
                } else {
                    super::super::interpolate(order, order.travelled)
                };
                let from = group.anchor.tile_floor();
                let to = next.tile_floor();
                if !self
                    .unit_step_clear(self.units[index].state.kind, from, to)
                    .unwrap_or(false)
                    || !self.building_step_clear(from, to)
                {
                    self.clear_planner(index);
                    self.units[index].route.clear();
                    order = MovementOrder {
                        origin: group.anchor,
                        waypoint: group.anchor,
                        travelled: 0,
                        segment_length: 0,
                        speed_carry: 0,
                        ..order
                    };
                    self.units[index].planning_since = self.tick.0;
                    self.units[index].fast_route_tried = false;
                    self.units[index].state.planning = true;
                    break;
                }
                group.anchor = next;
                group.progress += i64::from(step);
                if !arrived {
                    order.speed_carry = (credit % u128::from(denom)) as u64;
                    self.units[index].state.planning = false;
                    break;
                }
                if order.waypoint == order.destination && self.units[index].route.is_empty() {
                    self.clear_planner(index);
                    self.units[index].order = None;
                    self.units[index].state.planning = false;
                    self.formations.insert(leader, group);
                    return true;
                }
                let next = self.units[index]
                    .route
                    .pop_front()
                    .map(SegmentAdvance::Next)
                    .or_else(|| {
                        (order.waypoint != order.destination
                            && order.waypoint.tile_floor() == order.target_tile)
                            .then_some(SegmentAdvance::Next(order.destination))
                    })
                    .unwrap_or_else(|| self.next_map_segment(index, order));
                match next {
                    SegmentAdvance::Planning => {
                        self.units[index].state.planning = true;
                        self.units[index].planning_since = self.tick.0;
                        break;
                    }
                    SegmentAdvance::Stopped(error) => {
                        self.stop_route(index, error);
                        self.formations.insert(leader, group);
                        return true;
                    }
                    SegmentAdvance::Next(next) => {
                        let heading = WorldPosition::new(
                            next.x - order.waypoint.x,
                            next.y - order.waypoint.y,
                        );
                        let dx = i64::from(heading.x);
                        let dy = i64::from(heading.y);
                        group.travel_heading = heading;
                        order = MovementOrder {
                            origin: order.waypoint,
                            waypoint: next,
                            travelled: 0,
                            segment_length: segment_length(dx, dy),
                            speed_carry: 0,
                            ..order
                        };
                        self.refresh_group_path(index, &mut group, order);
                        self.units[index].state.planning = false;
                        if credit < u128::from(denom) {
                            order.speed_carry = credit as u64;
                            break;
                        }
                    }
                }
            }
            self.units[index].order = Some(order);
            still_moving.push(leader);
        }
        self.formations.insert(leader, group);
        true
    }
}

impl GameWorld {
    pub(super) fn formation_cohesive(&self, group: &Group) -> bool {
        // An attack-move group meets enemies on the way: bodies in its path
        // hold marching members back, and the blend toward the destination
        // layout makes the outermost slot run away at exactly the catch-up
        // pace. Waiting for them would slow the whole army at every contact.
        if group.direct || group.attack_move {
            return true;
        }
        let (x, y) = (
            i64::from(group.travel_heading.x),
            i64::from(group.travel_heading.y),
        );
        let span = (x * x + y * y).isqrt().max(1);
        // Hard-body files still need close cohesion while squeezing past
        // real obstacles. A nearby edge outside the block is not a squeeze.
        let mut cursor = group.path_cursor;
        let near_terrain = [-8, -4, 0, 4, 8].into_iter().any(|delta| {
            let at = (group.progress + delta * 1024).clamp(0, group.path.end());
            let (point, _) = group.path.point(at, &mut cursor);
            self.static_clearance(UnitKind::Militia, point.tile_floor()) < 15
        });
        let constrained = group.tight_destination()
            || (near_terrain && !self.formation_layout_clear(group, group.travel_heading));
        group.members.iter().all(|member| {
            self.group_work.members(1);
            if member.arrived || member.detached || member.blocked >= 240 {
                return true;
            }
            let unit = self.units[self.lookup[&member.id]].state;
            let position = unit.position;
            let obstructed = !constrained
                && !group
                    .static_area
                    .is_some_and(|area| area.covers(position, unit.kind.collision_radius(), 0))
                && self.static_clearance(unit.kind, position.tile_floor()) < 15
                && !self.body_step_clear(
                    unit.kind,
                    position,
                    path::step(position, member.goal, 2 * 1024),
                );
            if constrained || obstructed {
                path::squared(member.ideal, position) <= 512_i64.pow(2)
            } else {
                i64::from(member.ideal.x - position.x) * x
                    + i64::from(member.ideal.y - position.y) * y
                    <= 2 * 1024 * span
            }
        })
    }

    fn refresh_group_path(&self, index: usize, group: &mut Group, order: MovementOrder) {
        let end = group.progress + i64::from(order.segment_length.saturating_sub(order.travelled));
        let mut cursor = group.path_cursor;
        if group.path.end() < end || group.path.point(end, &mut cursor).0 != order.waypoint {
            group.path.continue_from(
                group.progress,
                group.anchor,
                std::iter::once(order.waypoint).chain(self.units[index].route.iter().copied()),
            );
        }
        group.path.point(group.progress, &mut group.path_cursor);
    }
}
