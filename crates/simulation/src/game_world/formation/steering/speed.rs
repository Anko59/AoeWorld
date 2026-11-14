//! Physical pace and conservative movement certificates for ordinary groups.
use super::*;

impl GameWorld {
    /// Every member's earned distance is at most ceil(1.5 * pace), capped
    /// at 512. Integer direction rounding adds less than one subunit; two
    /// extra subunits make the ordered-batch separation certificate strict.
    pub(crate) fn formation_step_bound(&self) -> i32 {
        self.formations
            .values()
            .filter(|group| !group.settled)
            .map(|group| {
                let mut bound =
                    (u128::from(group.pace.0) * 3).div_ceil(u128::from(group.pace.1) * 2);
                if let Some(first) = group.members.first() {
                    let unit = self.units[self.lookup[&first.id]].state;
                    if group.members.iter().all(|member| {
                        let other = self.units[self.lookup[&member.id]].state;
                        self.group_work.members(1);
                        other.kind == unit.kind && other.player == unit.player
                    }) {
                        // Every member has this current physical cap, including
                        // upgrades after anchor arrival. Mixed groups retain
                        // the conservative 1.5x-pace bound.
                        let (mut speed, denominator) = self.movement_speed(unit.kind, unit.player);
                        // Members can stand on different tiles: cover the fastest
                        // possible road step, even if the first member is on grass.
                        if !self.economy.roads.is_empty() && !unit.kind.is_naval() {
                            speed += speed / 4;
                        }
                        bound = bound.min(u128::from(speed.div_ceil(denominator)));
                    }
                }
                bound.min(512) as i32 + 2
            })
            .max()
            .unwrap_or(2)
    }

    pub(crate) fn formation_motion(&self, index: usize) -> Option<Motion> {
        let unit = &self.units[index];
        self.group_work.members(1);
        let leader = unit.formation?;
        let group = self.formations.get(&leader)?;
        let member = &group.members[group.member_index(unit.state.id)?];
        if member.detached {
            return None;
        }
        let navigating = self.units[self.lookup[&leader]].order.is_some();
        let own = self.movement_speed(unit.state.kind, unit.state.player);
        // Catch-up belongs to a trailing body. A front body cannot spend it
        // to race an explicitly coordinated anchor and then wait at a waypoint.
        let own = if member.progress >= group.progress + i64::from(member.offset.y) {
            group.limited_speed(own)
        } else {
            own
        };
        let credit = member.carry
            + matched_speed(
                group.pace,
                self.road_speed(unit.state.kind, unit.state.position, own),
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
            neighbor_radius: 512,
            remaining: 0,
            near_center: false,
            certified: false,
            static_clear: group.static_covers(unit.state.position, unit.state.kind),
        })
    }
}
