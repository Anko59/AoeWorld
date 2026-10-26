//! Fill an enclosed empty slot by exchanging assignments with its outer neighbour.
//! No member is pushed: both units walk their new slots under swept clearance.
use super::*;

fn squared(a: WorldPosition, b: WorldPosition) -> i64 {
    let x = i64::from(a.x) - i64::from(b.x);
    let y = i64::from(a.y) - i64::from(b.y);
    x * x + y * y
}

impl GameWorld {
    pub(crate) fn local_unit_edge_clear(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
    ) -> bool {
        self.clear_edge(index, from, to, false)
    }

    pub(super) fn settle_formation_slots(&mut self, group: &mut Group, work: &mut u32) {
        if !self.tick.0.is_multiple_of(8) {
            return;
        }
        let count = group.members.len();
        for turn in 0..count {
            let at = (self.tick.0 as usize / 8 + turn) % count;
            let member = &group.members[at];
            let index = self.lookup[&member.id];
            let unit = self.units[index].state;
            let goal = layout::oriented(group.anchor, member.offset, group.heading);
            let remaining = squared(unit.position, goal);
            if remaining == 0
                || remaining > (8 * 1024_i64).pow(2)
                || self.local_unit_edge_clear(index, unit.position, goal)
            {
                continue;
            }
            let mut best = None;
            let mut spent = 0;
            let length = remaining.isqrt().max(1);
            let probe = WorldPosition::new(
                goal.x + ((i64::from(unit.position.x) - i64::from(goal.x)) * 512 / length) as i32,
                goal.y + ((i64::from(unit.position.y) - i64::from(goal.y)) * 512 / length) as i32,
            );
            for id in self.spacing_neighbours(probe, self.tick.0.wrapping_add(u64::from(unit.id.0)))
            {
                let Some(&other_at) = group.member_lookup.get(&id) else {
                    continue;
                };
                if other_at == at {
                    continue;
                }
                let other_index = self.lookup[&id];
                let other = self.units[other_index].state;
                let other_goal =
                    layout::oriented(group.anchor, group.members[other_at].offset, group.heading);
                let travel = squared(unit.position, other_goal);
                if other.position != other_goal
                    || travel >= remaining
                    || squared(other_goal, goal) > 1536 * 1536
                    || !self.local_unit_edge_clear(other_index, other.position, goal)
                {
                    continue;
                }
                if *work < 4 {
                    break;
                }
                let before = *work;
                let clear = self.formation_line_clear(other.position, goal, work);
                spent += before - *work;
                if clear
                    && best.is_none_or(|(distance, best_id, _)| (travel, id) < (distance, best_id))
                {
                    best = Some((travel, id, other_at));
                }
            }
            self.route_metrics.formation_steering_work += u64::from(spent);
            if let Some((_, _, other)) = best {
                let offset = group.members[at].offset;
                group.members[at].offset = group.members[other].offset;
                group.members[other].offset = offset;
                for member in [at, other] {
                    group.members[member].cache = None;
                    group.members[member].detour.clear();
                }
                // Keep the derived column order current without a tick-time sort.
                for member in &mut group.column_order {
                    if *member == at {
                        *member = other;
                    } else if *member == other {
                        *member = at;
                    }
                }
                return;
            }
        }
    }
}
