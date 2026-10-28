//! Slot assignment belongs to orders, never to the tick or arrival loop.
use super::*;

pub(super) fn class(kind: UnitKind) -> u8 {
    match kind {
        UnitKind::Monk | UnitKind::Mangonel | UnitKind::BatteringRam => 2,
        UnitKind::Archer
        | UnitKind::Crossbowman
        | UnitKind::Skirmisher
        | UnitKind::EliteSkirmisher => 1,
        _ => 0,
    }
}

impl Group {
    pub(super) fn forget_departures(&mut self) {
        self.member_lookup = self
            .members
            .iter()
            .enumerate()
            .map(|(at, member)| (member.id, at))
            .collect();
        let mut order = (0..self.members.len()).collect::<Vec<_>>();
        order.sort_unstable_by_key(|&at| {
            let member = &self.members[at];
            (
                member.offset.x,
                std::cmp::Reverse(member.offset.y),
                member.id,
            )
        });
        for (rank, at) in order.into_iter().enumerate() {
            self.members[at].rank = rank;
        }
    }
}

pub(super) fn packed(rank: usize, count: usize, files: usize, gap: i32) -> WorldPosition {
    let rows = count.div_ceil(files.max(1));
    let file = rank / rows;
    let row = rank % rows;
    WorldPosition::new(
        (2 * file as i32 - (files as i32 - 1)) * gap / 2,
        ((rows as i32 - 1) - 2 * row as i32) * gap / 2,
    )
}

impl GameWorld {
    pub(super) fn assign_group_slots(&self, group: &mut Group) {
        let count = group.members.len();
        let mut slots = layout::slots(group.form, count)
            .into_iter()
            .map(|p| WorldPosition::new(p.x * group.gap / 1040, p.y * group.gap / 1040))
            .collect::<Vec<_>>();
        let clearance = self.static_clearance(UnitKind::Militia, group.destination.tile_floor());
        let half_width = slots.iter().map(|p| p.x.abs()).max().unwrap_or(0) + group.gap / 2;
        if clearance < 15 && i32::from(clearance) * 512 < half_width {
            let files = (i32::from(clearance) * 1024 / group.narrow_gap).max(1) as usize;
            slots = (0..count)
                .map(|rank| packed(rank, count, files, group.narrow_gap))
                .collect();
        }
        let mut members = (0..count).collect::<Vec<_>>();
        members.sort_unstable_by_key(|&at| {
            let member = &group.members[at];
            let unit = self.units[self.lookup[&member.id]].state;
            let x = i128::from(group.heading.x);
            let y = i128::from(group.heading.y);
            (
                class(unit.kind),
                x * i128::from(unit.position.y) - y * i128::from(unit.position.x),
                std::cmp::Reverse(
                    x * i128::from(unit.position.x) + y * i128::from(unit.position.y),
                ),
                member.id,
            )
        });
        if group.form == Formation::Box {
            slots.sort_unstable_by_key(|p| {
                (
                    std::cmp::Reverse(p.x.abs().max(p.y.abs())),
                    std::cmp::Reverse(p.y),
                    p.x,
                )
            });
        } else {
            slots.sort_unstable_by_key(|p| (std::cmp::Reverse(p.y), p.x));
        }
        // Allocate class bands first, then lateral files. Preserve facing
        // order inside each file so a late member never has to pass a teammate
        // that already occupies its destination. This replaces arrival swaps.
        let mut assignments = Vec::with_capacity(count);
        let mut start = 0;
        while start < count {
            let role = class(
                self.units[self.lookup[&group.members[members[start]].id]]
                    .state
                    .kind,
            );
            let mut end = start + 1;
            while end < count
                && class(
                    self.units[self.lookup[&group.members[members[end]].id]]
                        .state
                        .kind,
                ) == role
            {
                end += 1;
            }
            slots[start..end].sort_unstable_by_key(|point| (point.x, std::cmp::Reverse(point.y)));
            let mut file = start;
            while file < end {
                let mut next = file + 1;
                while next < end && slots[next].x == slots[file].x {
                    next += 1;
                }
                members[file..next].sort_unstable_by_key(|&at| {
                    let unit = self.units[self.lookup[&group.members[at].id]].state;
                    (
                        std::cmp::Reverse(
                            i128::from(group.heading.x) * i128::from(unit.position.x)
                                + i128::from(group.heading.y) * i128::from(unit.position.y),
                        ),
                        unit.id,
                    )
                });
                assignments.extend(
                    members[file..next]
                        .iter()
                        .copied()
                        .zip(slots[file..next].iter().copied()),
                );
                file = next;
            }
            start = end;
        }
        let mut reserved = BTreeMap::<TileCoord, Vec<(WorldPosition, i32)>>::new();
        for (at, offset) in assignments {
            let member = &mut group.members[at];
            let kind = self.units[self.lookup[&member.id]].state.kind;
            let desired = layout::oriented(group.destination, offset, group.heading);
            let radius = kind.collision_radius();
            let free = |point: WorldPosition| {
                let tile = point.tile_floor();
                self.static_clearance(kind, tile) > 0
                    && self.body_position_clear(kind, point)
                    && (-1..=1).all(|dy| {
                        (-1..=1).all(|dx| {
                            reserved
                                .get(&TileCoord::new(tile.x + dx, tile.y + dy))
                                .is_none_or(|positions| {
                                    positions.iter().all(|&(other, other_radius)| {
                                        path::squared(point, other)
                                            >= i64::from(radius + other_radius + 8).pow(2)
                                    })
                                })
                        })
                    })
            };
            let goal = std::iter::once(desired)
                .chain(
                    [
                        (0, -1),
                        (1, -1),
                        (1, 0),
                        (1, 1),
                        (0, 1),
                        (-1, 1),
                        (-1, 0),
                        (-1, -1),
                    ]
                    .into_iter()
                    .map(|(x, y)| WorldPosition::new(desired.x + x * 1024, desired.y + y * 1024)),
                )
                .filter(|&point| free(point))
                .min_by_key(|&point| (path::squared(point, desired), point.x, point.y))
                .unwrap_or(desired);
            member.offset = offset;
            member.goal = goal;
            member.stalled = 0;
            member.remaining = i64::MAX;
            member.blocked = 0;
            let position = self.units[self.lookup[&member.id]].state.position;
            let (x, y) = (
                i64::from(group.travel_heading.x),
                i64::from(group.travel_heading.y),
            );
            let span = (x * x + y * y).isqrt().max(1);
            let along = (i64::from(position.x - group.anchor.x) * x
                + i64::from(position.y - group.anchor.y) * y)
                / span;
            member.start_offset = WorldPosition::new(
                ((-y * i64::from(position.x - group.anchor.x)
                    + x * i64::from(position.y - group.anchor.y))
                    / span) as i32,
                along as i32,
            );
            member.cursor = group.path.cursor_at(group.progress + along);
            member.progress = group.path.project(position, &mut member.cursor);
            member.detour_progress = member.progress;
            member.carry = 0;
            member.arrived = false;
            member.detached = false;
            reserved
                .entry(goal.tile_floor())
                .or_default()
                .push((goal, radius));
        }
        group.forget_departures();
        group.form_start = group.progress;
        group.form_distance = group
            .members
            .iter()
            .map(|member| path::distance(member.start_offset, member.offset) * 4)
            .max()
            .unwrap_or(0)
            .min(path::distance(group.anchor, group.destination) / 2);
        group.half_width = group
            .members
            .iter()
            .map(|m| m.offset.x.abs())
            .max()
            .unwrap_or(0)
            + group.gap / 2;
    }
}
