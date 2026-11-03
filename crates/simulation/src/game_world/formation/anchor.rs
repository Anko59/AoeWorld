use super::*;
use crate::game_movement::SegmentAdvance;
use crate::game_path::segment_length;

mod taut;

/// Known route kept ahead of the anchor, so the corridor can be shaped
/// across leg boundaries before the block reaches them.
const PREFETCH_DISTANCE: i64 = 64 * 1024;

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
        let (speed, front_near_end) = self.group_speed(&group);
        group.speed = speed;
        if let Some(mut order) = self.units[index].order {
            let (speed, denominator) = group.speed;
            if denominator != previous_denominator {
                // Carry is fractional distance. A change of slowest member
                // must not reinterpret the old denominator as whole steps.
                order.speed_carry = (u128::from(order.speed_carry) * u128::from(denominator)
                    / u128::from(previous_denominator)) as u64;
            }
            let denom = denominator * 64;
            // Reserve at most 1/8 pace for cohesion, in proportion to how far
            // members trail. A prolonged 3/4-pace squeeze alone exceeds the
            // 1.3x arrival budget; persistent outliers recover independently.
            let pace = speed * (64 - self.formation_brake(&group));
            group.pace = (pace, denom);
            self.prefetch_group_route(index, order, front_near_end, group.anchor);
            if let Some(mut points) = self
                .taut_route(index, &mut group, order)
                .map(VecDeque::from)
                && let Some(waypoint) = points.pop_front()
            {
                let heading =
                    WorldPosition::new(waypoint.x - group.anchor.x, waypoint.y - group.anchor.y);
                if heading != WorldPosition::new(0, 0) {
                    group.travel_heading = heading;
                }
                order.origin = group.anchor;
                order.waypoint = waypoint;
                order.travelled = 0;
                order.segment_length = segment_length(i64::from(heading.x), i64::from(heading.y));
                self.units[index].route = points;
            }
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
    /// Pace reserved for cohesion, in 64ths of the slowest member's speed.
    /// It scales with how far the rearmost tenth of the pending members
    /// trails its slot: nothing within two tiles, the full 1/8 from ten on
    /// the march. A few blocked stragglers never hold the march, and a long
    /// march does not wait for laggards that cannot close the gap anyway.
    /// Within a block length of the destination the full reserve needs only
    /// six tiles and also counts a slot's remaining sideways distance, so
    /// the ranks arrive together.
    pub(super) fn formation_brake(&self, group: &Group) -> u64 {
        if group.direct || group.attack_move {
            return 0;
        }
        if group.tight_destination() {
            // Files that end inside a passage cannot reorder there: hold the
            // whole reserve until every member is within half a tile.
            let loose = group.members.iter().any(|member| {
                self.group_work.members(1);
                !(member.arrived || member.detached || member.blocked >= 240)
                    && path::squared(
                        member.ideal,
                        self.units[self.lookup[&member.id]].state.position,
                    ) > 512_i64.pow(2)
            });
            return if loose { 8 } else { 0 };
        }
        let remaining = group.path.end() - group.progress;
        let arriving = group.path.points.last() == Some(&group.destination)
            && remaining <= i64::from(group.half_width) + 8 * 1024;
        let count = group.members.len();
        let packed = group.packing_files < count;
        let forming = (group.progress - group.form_start).clamp(0, group.form_distance);
        let mut trailing = Vec::with_capacity(count);
        for member in &group.members {
            self.group_work.members(1);
            if member.arrived || member.detached || member.blocked >= 240 {
                continue;
            }
            let along = if packed {
                i64::from(
                    slots::packed(member.rank, count, group.packing_files, group.narrow_gap).y,
                )
            } else if group.form_distance == 0 {
                i64::from(member.offset.y)
            } else {
                let (start, end) = (i64::from(member.start_offset.y), i64::from(member.offset.y));
                start + (end - start) * forming / group.form_distance
            };
            let mut behind = group.progress + along - member.progress;
            if arriving {
                let position = self.units[self.lookup[&member.id]].state.position;
                behind = behind.max(path::distance(position, member.goal) - remaining);
            }
            trailing.push(behind);
        }
        if trailing.is_empty() {
            return 0;
        }
        let rank = if arriving {
            0
        } else {
            trailing.len().div_ceil(10) - 1
        };
        let (_, &mut lag, _) = trailing.select_nth_unstable_by(rank, |a, b| b.cmp(a));
        let (free, ramp) = if arriving {
            (1024, 4 * 1024)
        } else {
            (2 * 1024, 8 * 1024)
        };
        ((lag - free).clamp(0, ramp) * 8 / ramp) as u64
    }

    /// Emit only already proven continuations. Keep a bounded FIFO ahead of
    /// the front ranks; the anchor must not make them extrapolate a partial leg.
    fn prefetch_group_route(
        &mut self,
        index: usize,
        order: MovementOrder,
        front_near_end: bool,
        anchor: WorldPosition,
    ) {
        let end = self.units[index]
            .route
            .back()
            .copied()
            .unwrap_or(order.waypoint);
        if !(front_near_end || path::distance(anchor, end) < PREFETCH_DISTANCE)
            || path::distance(order.origin, order.destination) < 30 * 1024
            || self.units[index].route.len() >= 64
            || !self.units[index]
                .planner
                .as_ref()
                .is_some_and(RoutePlanner::has_route_continuation)
        {
            return;
        }
        let mut prefix = std::mem::take(&mut self.units[index].route);
        if let SegmentAdvance::Next(next) = self.next_map_segment(
            index,
            MovementOrder {
                waypoint: end,
                ..order
            },
        ) {
            prefix.push_back(next);
            prefix.append(&mut self.units[index].route);
        }
        self.units[index].route = prefix;
    }

    fn refresh_group_path(&self, index: usize, group: &mut Group, order: MovementOrder) {
        let end = group.progress + i64::from(order.segment_length.saturating_sub(order.travelled));
        let mut cursor = group.path_cursor;
        if group.path.end() < end
            || group.path.point(end, &mut cursor).0 != order.waypoint
            || group.path.points.last()
                != Some(self.units[index].route.back().unwrap_or(&order.waypoint))
        {
            group.path.continue_from(
                group.progress,
                group.anchor,
                std::iter::once(order.waypoint).chain(self.units[index].route.iter().copied()),
            );
        }
        group.path.point(group.progress, &mut group.path_cursor);
    }
}
