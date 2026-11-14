//! An attack-move member that fought leaves its group. When its last target
//! is gone and nothing else is in reach, it marches on to its slot again.
use super::*;

/// How many empty looks, four ticks apart, end a fight for good.
const EMPTY_LOOKS: u8 = 2;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Resume {
    pub(crate) slot: WorldPosition,
    pub(crate) empty: u8,
}

impl Resume {
    pub(crate) fn new(slot: WorldPosition) -> Self {
        Self { slot, empty: 0 }
    }
}

impl GameWorld {
    pub(super) fn advance_resumes(&mut self, changed: &mut Vec<EntityId>) {
        if self.combat.resuming.is_empty() {
            return;
        }
        let ids = self.combat.resuming.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let Some(resume) = self.combat.resuming.get(&id).copied() else {
                continue;
            };
            let idle = self.unit(id).filter(|u| {
                u.hp > 0
                    && !u.moving
                    && !u.planning
                    && matches!(u.task, UnitTask::Idle)
                    && !self.combat.attackers.contains_key(&id)
                    && !self.embarked(id)
            });
            // A new order, a death or a fresh fight ends the wait.
            let Some(unit) = idle else {
                self.combat.resuming.remove(&id);
                continue;
            };
            if !(self.tick.0 + u64::from(id.0)).is_multiple_of(4) {
                continue;
            }
            // An enemy whose every melee spot is taken is not a fight to
            // stand and wait for: the member marches on, as the group does.
            if let Some(target) = self
                .probe_for_target(id)
                .filter(|&target| self.attack_spot_free(id, target))
            {
                self.begin_autonomous_attack(id, target, unit.position);
                changed.push(id);
            } else if resume.empty + 1 < EMPTY_LOOKS {
                if let Some(entry) = self.combat.resuming.get_mut(&id) {
                    entry.empty += 1;
                }
            } else {
                self.combat.resuming.remove(&id);
                let moved = self.issue_formation_move(
                    unit.player,
                    &[id],
                    Formation::Line,
                    resume.slot,
                    false,
                    true,
                );
                if moved.is_ok() {
                    changed.push(id);
                }
            }
        }
    }
}

impl GameWorld {
    /// Every army group as a save keeps it: posture, members, where it is
    /// going and the waypoints behind that.
    pub(crate) fn formation_records(&self) -> Vec<crate::game_world::snapshot::FormationRecord> {
        self.formations
            .values()
            .map(|group| crate::game_world::snapshot::FormationRecord {
                form: group.form,
                members: group.members.iter().map(|member| member.id).collect(),
                destination: group.destination,
                queue: group.queue.iter().copied().collect(),
                attack_move: group.attack_move,
                pace_limit: group.speed_limit,
            })
            .collect()
    }
}

impl GameWorld {
    /// Contact reactions share an offensive group's ordinary continuation.
    pub(crate) fn offensive_destination_slot(&self, id: EntityId) -> Option<WorldPosition> {
        let unit = &self.units[*self.lookup.get(&id)?];
        let group = self.formations.get(&unit.formation?)?;
        if !group.attack_move {
            return None;
        }
        let at = group.members.binary_search_by_key(&id, |m| m.id).ok()?;
        Some(group.members[at].goal)
    }

    /// A bounded look-ahead point on an admitted movement route. Controllers
    /// can issue ordinary intermediate orders without a second pathfinder.
    pub(crate) fn movement_checkpoint(
        &self,
        id: EntityId,
        ahead: i32,
    ) -> Option<(WorldPosition, WorldPosition)> {
        let unit = &self.units[*self.lookup.get(&id)?];
        if let Some(group) = unit
            .formation
            .and_then(|leader| self.formations.get(&leader))
        {
            let progress = (group.progress + i64::from(ahead)).min(group.path.end());
            if progress > group.progress {
                return Some(group.path.point(progress, &mut { group.path_cursor }));
            }
            return (group.path.points.last() == Some(&group.destination))
                .then_some((group.destination, group.travel_heading));
        }
        let order = unit.order?;
        if unit.state.planning {
            return None;
        }
        let mut from = unit.state.position;
        let mut left = i64::from(ahead);
        let mut heading = WorldPosition::new(order.waypoint.x - from.x, order.waypoint.y - from.y);
        for to in std::iter::once(order.waypoint).chain(unit.route.iter().copied()) {
            let length = path::distance(from, to);
            heading = WorldPosition::new(to.x - from.x, to.y - from.y);
            if length >= left {
                return Some((path::step(from, to, left as i32), heading));
            }
            left -= length;
            from = to;
        }
        Some((from, heading))
    }
}
