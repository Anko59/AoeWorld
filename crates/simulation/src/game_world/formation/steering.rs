//! One target preparation per group, then immutable member proposals through
//! the ordinary parallel movement phase. Arrival never changes another slot.
use super::packing::block_heading;
use super::*;
use crate::game_world::movement_step::settling::GIVE_UP_AFTER_TICKS;

mod speed;

impl GameWorld {
    #[inline(never)] // a symbol of its own for the Callgrind stage counters
    pub(crate) fn advance_formations(
        &mut self,
        changed: &mut Vec<EntityId>,
        scratch: &mut crate::game_world::movement_step::Scratch,
    ) {
        if self.formations.is_empty() {
            return;
        }
        let leaders = self.formations.keys().copied().collect::<Vec<_>>();
        let mut movers = Vec::new();
        // Temporarily detach the map so no parallel task can observe another
        // group's intermediate targets. Canonical apply order stays unchanged.
        {
            let _span = self
                .planning_probe
                .as_ref()
                .and_then(|p| p.movement_span("group_prepare"));
            let mut groups = std::mem::take(&mut self.formations);
            {
                let mut jobs = groups
                    .iter_mut()
                    .map(|(&id, group)| (id, group))
                    .collect::<Vec<_>>();
                crate::game_world::parallel::par_each_mut(&mut jobs, |(leader, group)| {
                    if !group.settled {
                        self.prepare_group_targets(*leader, group);
                    }
                });
            }
            self.formations = groups;
            for group in self.formations.values().filter(|group| !group.settled) {
                movers.extend(
                    group
                        .members
                        .iter()
                        .inspect(|_| self.group_work.members(1))
                        .filter(|m| !m.detached)
                        .map(|m| m.id),
                );
            }
        }
        scratch.group_phase = true;
        {
            let _span = self
                .planning_probe
                .as_ref()
                .and_then(|p| p.movement_span("group_propose"));
            self.propose_steps_into(&movers, 0, scratch);
        }
        let _apply_span = self
            .planning_probe
            .as_ref()
            .and_then(|p| p.movement_span("group_apply"));
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
            if group.queue.is_empty()
                && group
                    .members
                    .iter()
                    .inspect(|_| self.group_work.members(1))
                    .all(|member| member.arrived)
            {
                let index = self.lookup[&leader];
                self.clear_planner(index);
                self.units[index].order = None;
                self.units[index].route.clear();
                self.units[index].state.planning = false;
                self.active_movers.remove(leader);
                group.anchor = group.destination;
                group.progress = group.path.end();
            }
            if self.units[self.lookup[&leader]].order.is_none() {
                // A member on a detour or still short of its give-up is not done:
                // settling would stop the rejoin and retries that finish it.
                group.settled = group.members.iter().all(|m| {
                    !self.units[self.lookup[&m.id]].state.moving
                        && !m.detached
                        && (m.arrived || m.stalled >= GIVE_UP_AFTER_TICKS)
                        && !self.retry_pending(m)
                });
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
        let mut ahead = 15;
        let look = i64::from(group.half_width) + 8 * 1024;
        for delta in [-look, -look / 2, 0, look / 2, look] {
            let progress = (group.progress + delta).clamp(0, group.path.end());
            let (point, _) = group.path.point(progress, &mut cursor);
            let value = self.static_clearance(UnitKind::Militia, point.tile_floor());
            if delta == 0 {
                group.static_area = crate::game_world::movement_step::certificates::StaticArea::new(
                    point.tile_floor(),
                    value,
                );
            }
            clearance = clearance.min(value);
            ahead = if delta > 0 { ahead.min(value) } else { ahead };
        }
        // A long file can still be on a bridge deck after the anchor's window
        // has left it. Sample the corridor back to the rearmost member too, so
        // the block keeps its narrow files until the whole column is across
        // (a wider layout would aim members on the deck into the water).
        let rear = if clearance < 15 || group.packing_files < group.members.len() {
            group
                .members
                .iter()
                .filter(|m| !m.arrived && !m.detached)
                .inspect(|_| self.group_work.members(1))
                .map(|m| m.progress)
                .min()
                .unwrap_or(group.progress)
        } else {
            group.progress
        };
        let mut trailing = 15;
        let behind = group.progress - look;
        if rear + 1024 < behind {
            let span = behind - rear;
            let steps = (span / 1024).clamp(1, 16);
            let mut at = group.path.cursor_at(rear.max(0));
            for k in 0..=steps {
                let progress = (rear + span * k / steps).clamp(0, group.path.end());
                let (point, _) = group.path.point(progress, &mut at);
                if self.near_deck(point) {
                    trailing =
                        trailing.min(self.static_clearance(UnitKind::Militia, point.tile_floor()));
                }
            }
        }
        let heading = block_heading(group);
        let files = if (clearance < 15
            && i32::from(clearance) * 512 < group.half_width
            // Hysteresis: a packed block stays packed while a narrowing lies ahead.
            && ((group.packing_files < group.members.len()
                && i32::from(ahead) * 512 < group.half_width)
                || !self.formation_layout_clear(group, heading)))
            || (trailing < 15 && i32::from(trailing) * 512 < group.half_width)
        {
            // Clearance is measured at a tile centre; a staircase lane is
            // narrower than that, so one half tile is kept as a margin.
            (i32::from(clearance.min(trailing)) * 1024 / group.narrow_gap).max(1) as usize
        } else {
            group.members.len()
        };
        let count = group.members.len();
        self.assign_packing_ranks(group, files.min(count), heading);
        let rotation = layout::Rotation::new(heading);
        let forming = (group.progress - group.form_start).clamp(0, group.form_distance);
        let stretching = files < count && group.long_files;
        let last = group.path.points.len().saturating_sub(1);
        let end = group.path.points[last];
        let before = group.path.points[last.saturating_sub(1)];
        let final_heading = WorldPosition::new(end.x - before.x, end.y - before.y);
        let final_length = path::distance(before, end).max(1);
        let static_area = group.static_area;
        for member in &mut group.members {
            self.group_work.members(1);
            let unit = self.units[self.lookup[&member.id]].state;
            // Carrier arrival does not prove that a trailing body has passed
            // the route's bends. Keep its shared corridor until it catches up.
            let catching_up = group.progress - member.progress > i64::from(group.half_width) + 1024;
            member.following_corridor |= !navigating && !group.direct && catching_up;
            let blend = |start: i32, end: i32| {
                if group.form_distance == 0 {
                    end
                } else {
                    (i64::from(start) + i64::from(end - start) * forming / group.form_distance)
                        as i32
                }
            };
            let offset = if files < count {
                let packed = slots::packed(member.rank, count, files, group.narrow_gap);
                if group.long_files {
                    WorldPosition::new(
                        blend(member.start_offset.x, packed.x),
                        blend(member.start_offset.y, packed.y),
                    )
                } else {
                    packed
                }
            } else {
                WorldPosition::new(
                    blend(member.start_offset.x, member.offset.x),
                    blend(member.start_offset.y, member.offset.y),
                )
            };
            // Project the slot's centre rather than its displaced body. On an
            // inside bend, projecting the body alone can keep selecting the
            // preceding leg forever even after the anchor has passed the turn.
            let (_, local_heading) = if files < count || member.following_corridor {
                group
                    .path
                    .corridor_point(member.progress, &mut member.cursor)
            } else {
                group.path.point(member.progress, &mut member.cursor)
            };
            let local_rotation = if local_heading == heading {
                rotation
            } else {
                layout::Rotation::new(local_heading)
            };
            let center = if member.following_corridor {
                unit.position
            } else {
                local_rotation.oriented(unit.position, WorldPosition::new(-offset.x, 0))
            };
            let mut projected_cursor = member.cursor;
            let projected = group.path.project(center, &mut projected_cursor);
            if projected > member.progress {
                member.progress = projected;
                member.cursor = projected_cursor;
            }
            member.target_progress = (group.progress + i64::from(offset.y))
                .max(
                    member.progress
                        + if member.following_corridor && !stretching {
                            1024
                        } else {
                            0
                        },
                )
                .min(member.progress + 1024);
            let mut at = member.cursor;
            let (point, next_heading) = if files < count || member.following_corridor {
                group.path.corridor_point(member.target_progress, &mut at)
            } else {
                group.path.point(member.target_progress, &mut at)
            };
            let next_rotation = if next_heading == local_heading {
                local_rotation
            } else if next_heading == heading {
                rotation
            } else {
                layout::Rotation::new(next_heading)
            };
            // Every tile-centre edge admits this fixed inset lane. Keeping
            // its width fixed avoids steering across the bank on every change
            // of the circular clearance sample along a staircase route.
            let lane_x = if self.near_deck(unit.position) || self.near_deck(point) {
                // A single-lane deck has no lateral passing lane. At a bend,
                // a smoothed normal can otherwise push an edge slot off it.
                0
            } else if member.following_corridor {
                let lateral = (512 - unit.kind.terrain_radius() - 2).max(0);
                offset.x.clamp(-lateral, lateral)
            } else {
                offset.x
            };
            member.center = next_rotation.oriented(point, WorldPosition::new(lane_x, 0));
            // A body beside its lane cannot enter a one-lane deck diagonally:
            // before such a narrowing it first steps sideways onto the lane.
            if (files < count || member.following_corridor)
                && (self.static_clearance(unit.kind, member.center.tile_floor()) <= 1
                    || self.static_clearance(unit.kind, unit.position.tile_floor()) <= 1)
            {
                let mut here = member.cursor;
                let (point, tangent) = group.path.corridor_point(member.progress + 128, &mut here);
                let lane = layout::oriented(point, WorldPosition::new(lane_x, 0), tangent);
                if path::squared(lane, unit.position) > 192_i64.pow(2) {
                    member.center = lane;
                    // Arc progress is earned at the point actually steered
                    // to, never a tile further on while the body stands.
                    member.target_progress = member.progress + 128;
                }
            }
            // A rear slot ends before the centroid. Begin its final approach
            // at that slot's projection, rather than marching past it first.
            let goal_along = (i64::from(member.goal.x - end.x) * i64::from(final_heading.x)
                + i64::from(member.goal.y - end.y) * i64::from(final_heading.y))
                / final_length;
            let continuing = !group.queue.is_empty();
            let final_approach = !continuing
                && member.following_corridor
                && end == group.destination
                && member.progress >= group.path.end() + goal_along.min(0) - 1024;
            let ideal = rotation.oriented(group.anchor, offset);
            let forward_to_slot = i64::from(ideal.x - unit.position.x) * i64::from(heading.x)
                + i64::from(ideal.y - unit.position.y) * i64::from(heading.y);
            if member.following_corridor
                && !final_approach
                && files == count
                && !catching_up
                && forward_to_slot >= 0
                && self.static_clearance(unit.kind, unit.position.tile_floor()) == 15
                && self.static_clearance(unit.kind, ideal.tile_floor()) >= 3
            {
                member.following_corridor = false;
            }
            member.ideal = if !continuing && (!navigating || group.direct || final_approach) {
                member.goal
            } else {
                rotation.oriented(group.anchor, offset)
            };
            if final_approach {
                member.center = member.goal;
            }
            member.target =
                if !group.direct && (member.following_corridor || (navigating && files < count)) {
                    member.center
                } else {
                    member.ideal
                };
            // A lateral slot can cross the inside of a bend even though the
            // shared centreline is clear. Aim at the proven next checkpoint,
            // rather than repeatedly asking local avoidance to enter a tree.
            if !group.direct
                && !member.arrived
                && (member.following_corridor
                    || member.blocked >= 16
                    || (group.attack_move
                        && !static_area.is_some_and(|area| {
                            area.covers(member.target, unit.kind.collision_radius(), 513)
                        })))
                && !self.near_deck(member.target)
                && self.static_clearance(unit.kind, member.target.tile_floor()) <= 2
                && !self.body_position_clear(unit.kind, member.target)
            {
                let mut at = member.cursor;
                let point = group.path.corridor_point(member.target_progress, &mut at).0;
                if self.body_position_clear(unit.kind, point) {
                    member.target = point;
                    member.center = point;
                    member.following_corridor = true;
                }
            }
            // A wide body at a deck end cannot cut the corridor's corner over
            // water as a narrow one does. A blocked one aims at the first
            // corridor point ahead whose first half-tile step is clear.
            if !group.direct
                && !member.arrived
                && member.blocked >= 16
                && self.near_deck(unit.position)
                && !self.half_step_clear(unit.kind, unit.position, member.target)
            {
                for ahead in 1..=4_i64 {
                    let mut at = member.cursor;
                    let point = group
                        .path
                        .corridor_point(member.progress + ahead * 512, &mut at)
                        .0;
                    if self.half_step_clear(unit.kind, unit.position, point) {
                        member.target = point;
                        member.center = point;
                        member.following_corridor = true;
                        break;
                    }
                }
            }
            if navigating
                && !group.direct
                && !member.arrived
                && (!member.following_corridor || stretching)
                && !final_approach
            {
                // Keep advancing while forming; an ahead member may move
                // laterally but never walks back just to assemble its row.
                let (x, y) = (i64::from(heading.x), i64::from(heading.y));
                let length = (x * x + y * y).max(1);
                let dot = i64::from(member.target.x - unit.position.x) * x
                    + i64::from(member.target.y - unit.position.y) * y;
                if dot < 0 && files == count {
                    member.target.x -= (x * dot / length) as i32;
                    member.target.y -= (y * dot / length) as i32;
                }
                if path::distance(member.target, unit.position) <= 1 {
                    // A long column leaves room for its following rows while
                    // stretching. Ordinary compact blocks keep their 3/4 pace.
                    // A single file keeps the full pace so the exit of a
                    // one-lane passage never backs up into it.
                    let quarters = if files == 1 {
                        4
                    } else if stretching {
                        1
                    } else {
                        3
                    };
                    let advance = (u128::from(group.pace.0) * quarters
                        / (u128::from(group.pace.1) * 4))
                        .clamp(1, 512) as i64;
                    member.target_progress = member.progress + advance;
                    let (point, tangent) = group
                        .path
                        .corridor_point(member.target_progress, &mut member.cursor);
                    member.center =
                        layout::oriented(point, WorldPosition::new(offset.x, 0), tangent);
                    member.target = member.center;
                }
            }
        }
    }

    /// The first half tile of a straight step from `from` towards `to`.
    fn half_step_clear(&self, kind: UnitKind, from: WorldPosition, to: WorldPosition) -> bool {
        let length = path::distance(from, to).max(1);
        let step = length.min(512);
        let next = WorldPosition::new(
            from.x + (i64::from(to.x - from.x) * step / length) as i32,
            from.y + (i64::from(to.y - from.y) * step / length) as i32,
        );
        self.unit_position_step_clear(kind, from, next)
            .unwrap_or(false)
    }
}
