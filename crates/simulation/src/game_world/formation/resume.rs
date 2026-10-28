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
