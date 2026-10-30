//! Temporary traffic rows preserve physical order; final roles stay immutable.
use super::slots::packed;
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
        positions.sort_unstable_by_key(|&(_, forward, lateral, id)| {
            (std::cmp::Reverse(forward), lateral, id)
        });
        let mut slots = (0..count)
            .map(|rank| (rank, packed(rank, count, files, group.narrow_gap)))
            .collect::<Vec<_>>();
        slots.sort_unstable_by_key(|&(rank, p)| (std::cmp::Reverse(p.y), p.x, rank));
        let mut start = 0;
        while start < count {
            let mut end = start + 1;
            // File-major packing has unequal row populations when the last
            // file is short (seven actors / three files gives rows 3, 2, 2).
            while end < count && slots[end].1.y == slots[start].1.y {
                end += 1;
            }
            positions[start..end].sort_unstable_by_key(|&(_, _, lateral, id)| (lateral, id));
            for (&(at, _, _, _), &(rank, _)) in positions[start..end].iter().zip(&slots[start..end])
            {
                self.group_work.members(1);
                group.members[at].rank = rank;
            }
            start = end;
        }
    }
}
