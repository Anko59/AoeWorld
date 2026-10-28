//! Local steering never creates a route planner. Visibility probes have a
//! fixed radius and cache tile pairs; the shared breadcrumb trail handles bends.
use super::*;

pub(crate) const STEERING_WORK_PER_TICK: u32 = 16_384;

#[derive(Clone, Copy, Debug)]
pub(super) enum Goal {
    Slot,
    Squeeze,
    Trail(WorldPosition),
    Hold,
}

impl GameWorld {
    pub(crate) fn advance_formations(&mut self, changed: &mut Vec<EntityId>) {
        if self.formations.is_empty() {
            return;
        }
        let mut leaders = self.formations.keys().copied().collect::<Vec<_>>();
        let start = self.tick.0 as usize % leaders.len();
        leaders.rotate_left(start);
        let mut steering_work = super::STEERING_WORK_PER_TICK;
        for leader in leaders {
            let Some(mut group) = self.formations.remove(&leader) else {
                continue;
            };
            if group.settled {
                self.formations.insert(leader, group);
                continue;
            }
            let mut assembled = true;
            let (speed, denom) = group.speed;
            let navigating = self.units[self.lookup[&leader]].order.is_some();
            self.update_formation_column(&mut group, navigating, &mut steering_work);
            if !navigating && !group.column {
                self.settle_formation_slots(&mut group, &mut steering_work);
            }
            let column = if group.column {
                self.formation_column_targets(&group)
            } else {
                Vec::new()
            };
            for turn in 0..group.members.len() {
                let at = (self.tick.0 as usize * 64 + turn) % group.members.len();
                let member = &mut group.members[at];
                let Some(&index) = self.lookup.get(&member.id) else {
                    continue;
                };
                let unit = self.units[index].state;
                let desired = column.get(at).copied().unwrap_or_else(|| {
                    layout::oriented(group.anchor, member.offset, group.heading)
                });
                let key = (
                    unit.position.tile_floor(),
                    group.anchor.tile_floor(),
                    if group.column {
                        group.travel_heading
                    } else {
                        group.heading
                    },
                );
                let goal = match member.cache {
                    Some((a, b, h, goal)) if (a, b, h) == key => goal,
                    _ => {
                        // Half covers the longest supported open formation
                        // offset (a 4,096-member Staggered wing); retain the
                        // other half for breadcrumbs around blocked slots.
                        let allowance = steering_work.min(256);
                        let direct_allowance = allowance / 2;
                        let mut work = direct_allowance;
                        let goal = if self.formation_line_clear(unit.position, desired, &mut work) {
                            Goal::Slot
                        } else if !group.column
                            && self.formation_line_clear(unit.position, group.anchor, &mut work)
                        {
                            Goal::Squeeze
                        } else {
                            let nearest = group
                                .trail
                                .iter()
                                .enumerate()
                                .min_by_key(|(_, p)| {
                                    let dx = i64::from(p.x) - i64::from(unit.position.x);
                                    let dy = i64::from(p.y) - i64::from(unit.position.y);
                                    dx * dx + dy * dy
                                })
                                .map_or(0, |(n, _)| n);
                            // Reserve half the probe allowance for following the
                            // shared path. A failed long ray cannot starve a bend.
                            work += allowance - direct_allowance;
                            let start = (nearest + 4).min(group.trail.len() - 1);
                            let mut goal = Goal::Hold;
                            for _ in 0..group.trail.len() {
                                if work == 0 {
                                    break;
                                }
                                let at = (start + group.trail.len()
                                    - member.probe % group.trail.len())
                                    % group.trail.len();
                                member.probe = (member.probe + 1) % group.trail.len();
                                let p = group.trail[at];
                                if self.formation_line_clear(unit.position, p, &mut work) {
                                    goal = Goal::Trail(p);
                                    member.probe = 0;
                                    break;
                                }
                            }
                            goal
                        };
                        let spent = if matches!(goal, Goal::Slot | Goal::Squeeze) {
                            direct_allowance - work
                        } else {
                            allowance - work
                        };
                        steering_work -= spent;
                        self.route_metrics.formation_steering_work += u64::from(spent);
                        member.cache = if matches!(goal, Goal::Hold) {
                            None
                        } else {
                            Some((key.0, key.1, key.2, goal))
                        };
                        goal
                    }
                };
                let target = match goal {
                    Goal::Slot => desired,
                    Goal::Squeeze => group.anchor,
                    Goal::Trail(p) => p,
                    Goal::Hold => unit.position,
                };
                let credit = member.carry + speed;
                let budget = (credit / denom).min(512) as i32;
                member.carry = credit % denom;
                let next = self.formation_local_step(index, target, budget, &mut member.detour);
                if next == unit.position && next != target {
                    member.cache = None;
                }
                if next != unit.position {
                    self.units[index].state.previous_position = unit.position;
                    self.units[index].state.position = next;
                    let state = self.units[index].state;
                    self.track_unit_step(member.id, unit.position.tile_floor(), &state);
                    if self.units[index].bucket != ChunkCoord::from_position(next) {
                        self.move_bucket(index, ChunkCoord::from_position(next));
                    }
                    self.update_unit_vision(member.id);
                }
                self.observe_unit_progress(index);
                let at_goal = next == target;
                if at_goal && matches!(goal, Goal::Slot | Goal::Squeeze) {
                    self.units[index].obstruction = None;
                }
                if at_goal && matches!(goal, Goal::Trail(_) | Goal::Hold) {
                    member.cache = None;
                }
                assembled &= at_goal && !matches!(goal, Goal::Hold | Goal::Trail(_));
                let state = &mut self.units[index].state;
                state.moving = navigating || !at_goal;
                if member.id != leader {
                    state.planning = false;
                }
                state.task = if state.moving {
                    UnitTask::Moving
                } else {
                    UnitTask::Idle
                };
                state.facing = if !navigating && at_goal {
                    group.facing
                } else {
                    facing_for(
                        i64::from(next.x) - i64::from(unit.position.x),
                        i64::from(next.y) - i64::from(unit.position.y),
                        group.facing,
                    )
                };
                if *state != unit {
                    changed.push(member.id);
                }
            }
            // Assemble on the march once slots are within the local cohesion
            // radius; one unfinished slot must not freeze the whole route.
            group.ready |= assembled || self.formation_slots_close(&group);
            if !navigating {
                // Anchor arrival completes a waypoint even inside a passage.
                // At the final destination, settle in the current shape: a
                // blocked wide frontage must not keep a column active forever.
                group.settled = assembled;
                self.next_formation_order(leader, &mut group);
            }
            self.formations.insert(leader, group);
        }
    }

    fn formation_local_step(
        &mut self,
        index: usize,
        to: WorldPosition,
        budget: i32,
        detour: &mut VecDeque<WorldPosition>,
    ) -> WorldPosition {
        let from = self.units[index].state.position;
        while detour.front().is_some_and(|&p| p == from) {
            detour.pop_front();
        }
        let target = detour.front().copied().unwrap_or(to);
        let proposed = local_step(from, target, budget);
        let blocked = !self.local_unit_step_clear(index, from, proposed);
        if blocked {
            self.record_obstruction_towards(index, to);
        }
        if blocked
            && self.local_retry(self.units[index].state.id)
            && let Some(path) = self.local_unit_route(index, to)
            && !path.is_empty()
        {
            *detour = path;
        }
        let target = detour.front().copied().unwrap_or(to);
        self.avoid_unit_step(index, local_step(from, target, budget))
    }

    pub(super) fn formation_line_clear(
        &self,
        from: WorldPosition,
        to: WorldPosition,
        work: &mut u32,
    ) -> bool {
        if !self.config.valid_ground_position(to) {
            return false;
        }
        let a = from.tile_floor();
        let b = to.tile_floor();
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let count = dx.abs().max(dy.abs());
        if count > 96 {
            return false;
        }
        let mut previous = a;
        for n in 1..=count.max(1) {
            if *work == 0 {
                return false;
            }
            *work -= 1;
            let tile = TileCoord::new(a.x + dx * n / count.max(1), a.y + dy * n / count.max(1));
            if !self
                .unit_step_clear(UnitKind::Militia, previous, tile)
                .unwrap_or(false)
                || !self.building_step_clear(previous, tile)
            {
                return false;
            }
            previous = tile;
        }
        true
    }
}

fn local_step(from: WorldPosition, to: WorldPosition, budget: i32) -> WorldPosition {
    let dx = i64::from(to.x) - i64::from(from.x);
    let dy = i64::from(to.y) - i64::from(from.y);
    let length = (dx * dx + dy * dy).isqrt().max(1);
    let step = i64::from(budget).min(length);
    let x = (dx * step / length) as i32;
    let y = (dy * step / length) as i32;
    if step >= length {
        to
    } else {
        WorldPosition::new(from.x + x, from.y + y)
    }
}
