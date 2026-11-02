//! Temporary traffic rows preserve physical order; final roles stay immutable.
use super::*;

impl GameWorld {
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
