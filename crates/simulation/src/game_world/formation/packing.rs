//! Temporary traffic rows preserve physical order; final roles stay immutable.
use super::*;

impl GameWorld {
    pub(crate) fn finish_formation_member(
        &mut self,
        id: EntityId,
        motion: Motion,
        from: WorldPosition,
        to: WorldPosition,
    ) {
        let index = self.lookup[&id];
        self.group_work.members(1);
        let Some(group) = self.units[index]
            .formation
            .and_then(|leader| self.formations.get_mut(&leader))
        else {
            return;
        };
        let Some(at) = group.member_index(id) else {
            return;
        };
        let member = &mut group.members[at];
        // Only a body that moved earns the arc of the point it steered to: a
        // blocked body beside a bend must not run its progress ahead.
        if motion.near_center && from != to {
            member.progress = member.progress.max(motion.path_progress);
        }
        member.carry = motion.carry;
        let arrived = motion.final_leg && to == motion.goal;
        member.arrived |= arrived;
        let remaining = motion.remaining;
        let progress = remaining <= member.remaining.saturating_sub(512);
        member.stalled = if motion.navigating || arrived || progress {
            member.remaining = remaining;
            0
        } else {
            member.stalled.saturating_add(1)
        };
        let forward = member.progress >= member.detour_progress + 512;
        if forward {
            member.detour_progress = member.progress;
            if let Some(obstruction) = &mut self.units[index].obstruction {
                obstruction.blocked = 0;
            }
        }
        member.following_corridor |= motion.obstructed;
        member.blocked = if motion.obstructed && !forward && from != motion.target {
            member.blocked.saturating_add(1)
        } else {
            0
        };
        if arrived {
            self.units[index].obstruction = None;
        }
    }

    pub(super) fn assign_packing_ranks(
        &self,
        group: &mut Group,
        files: usize,
        heading: WorldPosition,
    ) {
        if group.packing_files == files {
            return;
        }
        group.packing_files = files;
        let count = group.members.len();
        // A pair of sixteen-row cohorts still uses ordinary compact steering.
        // Larger selections need an elastic approach once a file exceeds that
        // window. Preserve the decision across later clearance changes.
        group.long_files |= count > 32 && count.div_ceil(files.max(1)) > 16;
        let (x, y) = (i64::from(heading.x), i64::from(heading.y));
        let length = (x * x + y * y).isqrt().max(1);
        let mut positions = Vec::with_capacity(count);
        for (at, member) in group.members.iter_mut().enumerate() {
            self.group_work.members(1);
            let p = self.units[self.lookup[&member.id]].state.position;
            member.progress = group.path.relocate(p, &mut member.cursor);
            member.detour_progress = member.progress;
            if files >= count {
                let (dx, dy) = (
                    i64::from(p.x - group.anchor.x),
                    i64::from(p.y - group.anchor.y),
                );
                member.start_offset = WorldPosition::new(
                    ((-y * dx + x * dy) / length) as i32,
                    ((x * dx + y * dy) / length) as i32,
                );
            } else if group.long_files {
                let (point, tangent) = group
                    .path
                    .corridor_point(member.progress, &mut member.cursor);
                let (dx, dy) = (i64::from(p.x - point.x), i64::from(p.y - point.y));
                let (tx, ty) = (i64::from(tangent.x), i64::from(tangent.y));
                let length = (tx * tx + ty * ty).isqrt().max(1);
                member.start_offset = WorldPosition::new(
                    ((-ty * dx + tx * dy) / length) as i32,
                    (member.progress - group.progress) as i32,
                );
            }
            positions.push((
                at,
                i64::from(p.x) * x + i64::from(p.y) * y,
                i64::from(p.y) * x - i64::from(p.x) * y,
                member.id,
            ));
        }
        if files >= count {
            // Re-form from the bodies leaving the pass, not the old spawn
            // offsets. No abrupt return to the full layout at the exit.
            group.form_start = group.progress;
            group.form_distance = group
                .members
                .iter()
                .map(|m| path::distance(m.start_offset, m.offset) * 4)
                .max()
                .unwrap_or(0)
                .min(path::distance(group.anchor, group.destination) / 2);
            return;
        }
        // Files take members in lateral order, so a narrowing block only
        // closes towards its centre line and no member crosses a neighbour's
        // file. Each file keeps its own forward order.
        positions.sort_unstable_by_key(|&(_, forward, lateral, id)| {
            (lateral, std::cmp::Reverse(forward), id)
        });
        let rows = count.div_ceil(files);
        for (file, chunk) in positions.chunks_mut(rows).enumerate() {
            chunk.sort_unstable_by_key(|&(_, forward, _, id)| (std::cmp::Reverse(forward), id));
            for (row, &(at, _, _, _)) in chunk.iter().enumerate() {
                self.group_work.members(1);
                group.members[at].rank = file * rows + row;
            }
        }
        if !group.long_files {
            return;
        }
        // Turning a compact crowd into a long column needs room to stretch.
        // Reserve its file order now, but blend from each physical lane rather
        // than pulling all rows into their narrow files in one tick.
        let low = group
            .members
            .iter()
            .map(|m| m.start_offset.y)
            .min()
            .unwrap_or(0);
        let high = group
            .members
            .iter()
            .map(|m| m.start_offset.y)
            .max()
            .unwrap_or(0);
        let column = (rows.saturating_sub(1) as i64) * i64::from(group.narrow_gap);
        group.form_start = group.progress;
        group.form_distance = if column > i64::from(high - low) + 2 * 1024 {
            group
                .members
                .iter()
                .map(|m| {
                    path::distance(
                        m.start_offset,
                        slots::packed(m.rank, count, files, group.narrow_gap),
                    ) * 2
                })
                .max()
                .unwrap_or(0)
                .min(path::distance(group.anchor, group.destination) / 2)
        } else {
            0
        };
    }
}

/// The block turns from its travel heading towards the order heading over
/// the last block length before the destination.
pub(super) fn block_heading(group: &Group) -> WorldPosition {
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
