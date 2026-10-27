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
        group.speed = self.group_speed(&group);
        if let Some(mut order) = self.units[index].order {
            let (speed, denom) = group.speed;
            // A stretched column slows the anchor while members catch up.
            // Stopping it completely strands the front on intermediate slots
            // and lets one delayed member halt the whole formation forever.
            let pace = if group.ready && self.formation_cohesive(&group) {
                speed
            } else {
                speed / 2
            };
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
                if group.trail.back().is_none_or(|p| p.tile_floor() != to) {
                    if group.trail.len() == 128 {
                        group.trail.pop_front();
                    }
                    group.trail.push_back(next);
                }
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
        if group.column {
            let mut previous = group.anchor;
            return group.column_order.iter().all(|&at| {
                let m = &group.members[at];
                self.unit(m.id).is_none_or(|u| {
                    let dx = i64::from(u.position.x) - i64::from(previous.x);
                    let dy = i64::from(u.position.y) - i64::from(previous.y);
                    previous = u.position;
                    dx * dx + dy * dy <= (6 * 1024_i64).pow(2)
                })
            });
        }
        group.members.iter().all(|m| {
            self.unit(m.id).is_none_or(|u| {
                let dx = i64::from(u.position.x) - i64::from(group.anchor.x);
                let dy = i64::from(u.position.y) - i64::from(group.anchor.y);
                let x = i64::from(m.offset.x);
                let y = i64::from(m.offset.y);
                let radius = (x * x + y * y).isqrt() + 8 * 1024;
                dx * dx + dy * dy <= radius * radius
            })
        })
    }
}
