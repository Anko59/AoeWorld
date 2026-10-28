//! One target preparation per group, then immutable member proposals through
//! the ordinary parallel movement phase. Arrival never changes another slot.
use super::*;
use crate::game_world::movement_step::settling::GIVE_UP_AFTER_TICKS;

const FRACTION: u64 = 1 << 32;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Motion {
    pub(crate) target: WorldPosition,
    pub(crate) center: WorldPosition,
    pub(crate) goal: WorldPosition,
    pub(crate) budget: i32,
    pub(crate) carry: u64,
    pub(crate) navigating: bool,
    pub(crate) final_leg: bool,
    pub(crate) done: bool,
    pub(crate) facing: Facing,
    pub(crate) obstructed: bool,
    pub(crate) path_progress: i64,
}

pub(crate) fn matched_speed(pace: (u64, u64), own: (u64, u64)) -> u64 {
    let catch_up = u128::from(pace.0) * u128::from(FRACTION) * 5 / (u128::from(pace.1) * 4);
    let cap = u128::from(own.0) * u128::from(FRACTION) / u128::from(own.1);
    catch_up.min(cap) as u64
}

impl GameWorld {
    pub(crate) fn advance_formations(
        &mut self,
        changed: &mut Vec<EntityId>,
        scratch: &mut crate::game_world::movement_step::Scratch,
    ) {
        if self.formations.is_empty() {
            return;
        }
        self.rejoin_detached_members();
        let leaders = self.formations.keys().copied().collect::<Vec<_>>();
        let mut movers = Vec::new();
        for &leader in &leaders {
            let Some(mut group) = self.formations.remove(&leader) else {
                continue;
            };
            if !group.settled {
                self.prepare_group_targets(leader, &mut group);
                movers.extend(group.members.iter().filter(|m| !m.detached).map(|m| m.id));
            }
            self.formations.insert(leader, group);
        }
        // The BTreeMap and canonical member storage give group-then-unit order.
        scratch.group_phase = true;
        self.propose_steps_into(&movers, 0, scratch);
        let mut unused = Vec::new();
        for turn in 0..movers.len() {
            if let Some(step) = scratch.take_step(turn) {
                self.apply_step(step, &mut unused, changed);
            }
        }
        scratch.group_phase = false;
        for leader in leaders {
            let Some(mut group) = self.formations.remove(&leader) else {
                continue;
            };
            if group.queue.is_empty() && group.members.iter().all(|member| member.arrived) {
                let index = self.lookup[&leader];
                self.clear_planner(index);
                self.units[index].order = None;
                self.units[index].route.clear();
                self.units[index].state.planning = false;
                if let Ok(at) = self.active_movers.binary_search(&leader) {
                    self.active_movers.remove(at);
                }
                group.anchor = group.destination;
                group.progress = group.path.end();
            }
            if self.units[self.lookup[&leader]].order.is_none() {
                group.settled = group
                    .members
                    .iter()
                    .all(|m| !self.units[self.lookup[&m.id]].state.moving);
                self.next_formation_order(leader, &mut group);
            }
            self.formations.insert(leader, group);
        }
        self.detach_blocked_members();
    }

    fn prepare_group_targets(&self, leader: EntityId, group: &mut Group) {
        let navigating = self.units[self.lookup[&leader]].order.is_some();
        let mut cursor = group.path_cursor;
        // Look ahead and behind the anchor before the front reaches a wall.
        // A capped value proves at least 7.5 tiles; it is not a wall at 7.5.
        let mut clearance = 15;
        for delta in [-8, -4, 0, 4, 8] {
            let progress = (group.progress + delta * 1024).clamp(0, group.path.end());
            let (point, _) = group.path.point(progress, &mut cursor);
            clearance = clearance.min(self.static_clearance(UnitKind::Militia, point.tile_floor()));
        }
        let files = if clearance < 15 && i32::from(clearance) * 512 < group.half_width {
            (i32::from(clearance) * 1024 / group.narrow_gap).max(1) as usize
        } else {
            group.members.len()
        };
        let count = group.members.len();
        let heading = block_heading(group);
        let forming = (group.progress - group.form_start).clamp(0, group.form_distance);
        for member in &mut group.members {
            let unit = self.units[self.lookup[&member.id]].state;
            let offset = if files < count {
                slots::packed(member.rank, count, files, group.narrow_gap)
            } else {
                let blend = |start: i32, end: i32| {
                    if group.form_distance == 0 {
                        end
                    } else {
                        (i64::from(start) + i64::from(end - start) * forming / group.form_distance)
                            as i32
                    }
                };
                WorldPosition::new(
                    blend(member.start_offset.x, member.offset.x),
                    blend(member.start_offset.y, member.offset.y),
                )
            };
            // Project the slot's centre rather than its displaced body. On an
            // inside bend, projecting the body alone can keep selecting the
            // preceding leg forever even after the anchor has passed the turn.
            let (_, local_heading) = group.path.point(member.progress, &mut member.cursor);
            let center = layout::oriented(
                unit.position,
                WorldPosition::new(-offset.x, 0),
                local_heading,
            );
            let mut projected_cursor = member.cursor;
            let projected = group.path.project(center, &mut projected_cursor);
            if projected > member.progress {
                member.progress = projected;
                member.cursor = projected_cursor;
            }
            member.target_progress = (group.progress + i64::from(offset.y))
                .max(member.progress)
                .min(member.progress + 1024);
            let mut at = member.cursor;
            let (point, local_heading) = group.path.point(member.target_progress, &mut at);
            member.center = layout::oriented(point, WorldPosition::new(offset.x, 0), local_heading);
            member.ideal = if !navigating || group.direct {
                member.goal
            } else {
                layout::oriented(group.anchor, offset, heading)
            };
            member.target = member.ideal;
            if navigating && !group.direct && !member.arrived {
                // Keep advancing while forming; an ahead member may move
                // laterally but never walks back just to assemble its row.
                let (x, y) = (i64::from(heading.x), i64::from(heading.y));
                let length = (x * x + y * y).max(1);
                let dot = i64::from(member.target.x - unit.position.x) * x
                    + i64::from(member.target.y - unit.position.y) * y;
                if dot < 0 {
                    member.target.x -= (x * dot / length) as i32;
                    member.target.y -= (y * dot / length) as i32;
                }
            }
        }
    }

    pub(crate) fn formation_motion(&self, index: usize) -> Option<Motion> {
        let unit = &self.units[index];
        let leader = unit.formation?;
        let group = self.formations.get(&leader)?;
        let member = &group.members[*group.member_lookup.get(&unit.state.id)?];
        if member.detached {
            return None;
        }
        let navigating = self.units[self.lookup[&leader]].order.is_some();
        let credit = member.carry
            + matched_speed(
                group.pace,
                self.movement_speed(unit.state.kind, unit.state.player),
            );
        Some(Motion {
            target: member.target,
            center: member.center,
            goal: member.goal,
            budget: (credit / FRACTION).min(512) as i32,
            carry: credit % FRACTION,
            navigating,
            final_leg: group.queue.is_empty(),
            done: member.arrived || (!navigating && member.stalled >= GIVE_UP_AFTER_TICKS),
            facing: group.facing,
            obstructed: false,
            path_progress: member.target_progress,
        })
    }

    pub(crate) fn finish_formation_member(
        &mut self,
        id: EntityId,
        motion: Motion,
        from: WorldPosition,
        to: WorldPosition,
    ) {
        let index = self.lookup[&id];
        let Some(group) = self.units[index]
            .formation
            .and_then(|leader| self.formations.get_mut(&leader))
        else {
            return;
        };
        let Some(&at) = group.member_lookup.get(&id) else {
            return;
        };
        let member = &mut group.members[at];
        if path::squared(to, motion.center) <= 512_i64.pow(2) {
            member.progress = member.progress.max(motion.path_progress);
        }
        member.carry = motion.carry;
        let arrived = motion.final_leg && to == motion.goal;
        member.arrived |= arrived;
        let remaining = path::distance(to, motion.goal);
        let progress = remaining <= member.remaining.saturating_sub(512);
        member.stalled = if motion.navigating || arrived || progress {
            member.remaining = remaining;
            0
        } else {
            member.stalled.saturating_add(1)
        };
        let forward = member.progress >= member.detour_progress + 512;
        member.blocked = if motion.obstructed && !forward && from != motion.target {
            member.blocked.saturating_add(1)
        } else {
            member.detour_progress = member.progress;
            0
        };
        if arrived {
            self.units[index].obstruction = None;
        }
    }
}

fn block_heading(group: &Group) -> WorldPosition {
    let remaining = (group.path.end() - group.progress).max(0);
    let radius = i64::from(group.half_width) + 2 * 1024;
    if group.path.points.last() != Some(&group.destination) || remaining >= radius {
        return group.travel_heading;
    }
    let normal = |point: WorldPosition| {
        let (x, y) = (i64::from(point.x), i64::from(point.y));
        let length = (x * x + y * y).isqrt().max(1);
        (x * 1024 / length, y * 1024 / length)
    };
    let (a, b) = (normal(group.travel_heading), normal(group.heading));
    WorldPosition::new(
        ((a.0 * remaining + b.0 * (radius - remaining)) / radius) as i32,
        ((a.1 * remaining + b.1 * (radius - remaining)) / radius) as i32,
    )
}
