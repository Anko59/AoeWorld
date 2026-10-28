//! A blocked frontage follows spaced predecessors along the leader's route.
use super::*;

impl Group {
    pub(super) fn assign_offsets(&mut self, positions: &[WorldPosition]) {
        self.member_lookup = self
            .members
            .iter()
            .enumerate()
            .map(|(at, m)| (m.id, at))
            .collect();
        let x = i128::from(self.heading.x);
        let y = i128::from(self.heading.y);
        let mut members = (0..self.members.len()).collect::<Vec<_>>();
        members.sort_unstable_by_key(|&at| {
            let p = positions[at];
            (
                std::cmp::Reverse(x * i128::from(p.x) + y * i128::from(p.y)),
                x * i128::from(p.y) - y * i128::from(p.x),
                self.members[at].id,
            )
        });
        let mut slots = self.members.iter().map(|m| m.offset).collect::<Vec<_>>();
        slots.sort_unstable_by_key(|p| (std::cmp::Reverse(p.y), p.x));
        for (at, slot) in members.into_iter().zip(slots) {
            self.members[at].offset = slot;
            self.members[at].cache = None;
            self.members[at].detour.clear();
        }
        self.refresh_column_order();
    }

    /// After members left: rebuild the id lookup and the derived column order.
    pub(super) fn forget_departures(&mut self) {
        self.member_lookup = self
            .members
            .iter()
            .enumerate()
            .map(|(at, m)| (m.id, at))
            .collect();
        self.refresh_column_order();
    }

    pub(super) fn refresh_column_order(&mut self) {
        // Choose the front's centre before its wings. Sorting belongs to order
        // issuance/shape changes; ticks only walk this fixed order in O(n).
        self.column_order = (0..self.members.len()).collect();
        self.column_order.sort_unstable_by_key(|&at| {
            let member = &self.members[at];
            (
                std::cmp::Reverse(member.offset.y),
                member.offset.x.unsigned_abs(),
                member.offset.x,
                member.id,
            )
        });
    }
}

impl GameWorld {
    pub(super) fn update_formation_column(
        &mut self,
        group: &mut Group,
        navigating: bool,
        work: &mut u32,
    ) {
        let mut anchor = group.anchor;
        if navigating {
            let x = i64::from(group.travel_heading.x);
            let y = i64::from(group.travel_heading.y);
            let length = (x * x + y * y).isqrt().max(1);
            anchor.x = anchor.x.saturating_add((x * 4096 / length) as i32);
            anchor.y = anchor.y.saturating_add((y * 4096 / length) as i32);
        }
        let before = (*work).min(4096);
        let mut probes = before;
        let mut blocked = false;
        let mut complete = true;
        for member in &group.members {
            let Some(unit) = self.unit(member.id) else {
                continue;
            };
            let desired = layout::oriented(anchor, member.offset, group.heading);
            let a = unit.position.tile_floor();
            let b = desired.tile_floor();
            let distance = a.x.abs_diff(b.x).max(a.y.abs_diff(b.y)).max(1);
            // An unknown long ray must not collapse a wide army on open land.
            if distance > 96 || distance > probes {
                complete = false;
                continue;
            }
            if !self.formation_line_clear(unit.position, desired, &mut probes) {
                blocked = true;
                break;
            }
        }
        let spent = before - probes;
        *work -= spent;
        self.route_metrics.formation_steering_work += u64::from(spent);
        let column = blocked || (group.column && !complete);
        if column != group.column {
            group.column = column;
            for member in &mut group.members {
                member.cache = None;
                member.detour.clear();
            }
        }
        // A group already inside a pass cannot assemble a wide frontage first.
        group.ready |= group.column;
    }

    pub(super) fn formation_column_targets(&self, group: &Group) -> Vec<WorldPosition> {
        let mut previous = group.anchor;
        let dx = i64::from(group.travel_heading.x);
        let dy = i64::from(group.travel_heading.y);
        let length = (dx * dx + dy * dy).isqrt().max(1);
        let gap_x = (dx * 1040 / length) as i32;
        let gap_y = (dy * 1040 / length) as i32;
        let mut targets = vec![group.anchor; group.members.len()];
        for (rank, &at) in group.column_order.iter().enumerate() {
            let Some(unit) = self.unit(group.members[at].id) else {
                continue;
            };
            targets[at] = if rank == 0 {
                group.anchor
            } else {
                WorldPosition::new(previous.x - gap_x, previous.y - gap_y)
            };
            previous = unit.position;
        }
        targets
    }

    pub(super) fn formation_slots_close(&self, group: &Group) -> bool {
        group.members.iter().all(|member| {
            self.unit(member.id).is_none_or(|unit| {
                let desired = layout::oriented(group.anchor, member.offset, group.heading);
                let x = i64::from(unit.position.x) - i64::from(desired.x);
                let y = i64::from(unit.position.y) - i64::from(desired.y);
                x * x + y * y <= (8 * 1024_i64).pow(2)
            })
        })
    }
}
