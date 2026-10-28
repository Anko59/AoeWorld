//! Idle acquisition and stance control; controller ownership is irrelevant.
use super::{MELEE_RANGE_SUBUNITS, targeting::TargetArea, tick::Goal};
use crate::{EconomyError, GameWorld, UnitKind, UnitTask};
use aoe_core::{CombatStance, EntityId, PlayerId, WorldPosition};
use std::ops::Bound::{Excluded, Unbounded};

pub(super) const MAX_ACQUISITIONS: usize = 128;
pub(crate) fn autonomous_kind(kind: UnitKind) -> bool {
    super::unit_combat(kind).attack > 0 && kind != UnitKind::Villager
}

impl GameWorld {
    pub fn issue_stance(
        &mut self,
        player: PlayerId,
        ids: &[EntityId],
        stance: CombatStance,
    ) -> Result<(), EconomyError> {
        if ids.is_empty() || ids.len() > aoe_core::MAX_FORMATION_MEMBERS {
            return Err(EconomyError::OrderTooLarge);
        }
        let mut seen = std::collections::BTreeSet::new();
        for &id in ids {
            if !seen.insert(id) {
                return Err(EconomyError::OrderTooLarge);
            }
            let u = self
                .unit(id)
                .filter(|u| u.hp > 0 && !self.embarked(id))
                .ok_or(EconomyError::UnknownEntity)?;
            if u.player != player {
                return Err(EconomyError::NotOwner);
            }
            if !autonomous_kind(u.kind) {
                return Err(EconomyError::WrongUnitKind);
            }
        }
        for &id in ids {
            if self
                .combat
                .attackers
                .get(&id)
                .is_some_and(|a| a.origin.is_some())
            {
                self.end_attack(id, &mut Vec::new());
            }
            self.units[self.lookup[&id]].state.stance = stance;
        }
        Ok(())
    }

    pub(super) fn advance_acquisition(&mut self, changed: &mut Vec<EntityId>) {
        let cursor = self.combat.seeker_cursor;
        let ids: Vec<_> = self
            .combat
            .seekers
            .range((Excluded(cursor), Unbounded))
            .chain(self.combat.seekers.range(..=cursor))
            .copied()
            .take(MAX_ACQUISITIONS)
            .collect();
        for id in ids {
            if self.combat.stats.targeting_scanned >= super::targeting::MAX_TARGET_INSPECTIONS / 2 {
                break;
            }
            self.combat.seeker_cursor = id;
            self.combat.stats.acquisition_checks += 1;
            let Some(u) = self.unit(id) else {
                continue;
            };
            let building_attack = self
                .combat
                .attackers
                .get(&id)
                .copied()
                .filter(|a| a.origin.is_some() && self.building(a.target).is_some());
            if u.hp == 0
                || (u.moving && building_attack.is_none())
                || (u.planning && building_attack.is_none())
                || u.auto_scout
                || u.stance == CombatStance::NoAttack
                || (!matches!(u.task, UnitTask::Idle) && building_attack.is_none())
                || self.embarked(id)
            {
                continue;
            }
            let Some(area) = self.acquisition_area(&u) else {
                continue;
            };
            if let Some(target) = self.acquire_target(&area, building_attack.is_none()) {
                if building_attack.is_some() {
                    self.stop_movement(self.lookup[&id]);
                }
                self.begin_autonomous_attack(
                    id,
                    target,
                    building_attack.and_then(|a| a.origin).unwrap_or(u.position),
                );
                changed.push(id);
            }
        }
    }

    /// The sight and range window within which an idle unit looks for enemies.
    pub(super) fn acquisition_area(&self, u: &crate::GameUnit) -> Option<TargetArea> {
        let (_, _, los) = self.unit_observer(u)?;
        let stats = self.unit_stats(u.id)?;
        let range = super::super::navy::attack_range(&stats);
        let max = if u.stance == CombatStance::StandGround {
            range.min(i64::from(los) * MELEE_RANGE_SUBUNITS)
        } else {
            i64::from(los) * MELEE_RANGE_SUBUNITS
        };
        Some(TargetArea {
            player: u.player,
            source: Goal::Unit(u.position, u.kind.collision_radius()),
            min: i64::from(stats.min_range) * MELEE_RANGE_SUBUNITS,
            max,
            actor: u.id,
            ram: u.kind == UnitKind::BatteringRam,
        })
    }

    /// One bounded look for an enemy, as an idle unit would make.
    pub(crate) fn probe_for_target(&mut self, id: EntityId) -> Option<EntityId> {
        let u = self.unit(id)?;
        if u.stance == CombatStance::NoAttack || u.auto_scout {
            return None;
        }
        let area = self.acquisition_area(&u)?;
        self.acquire_target(&area, true)
    }

    pub(crate) fn begin_autonomous_attack(
        &mut self,
        id: EntityId,
        target: EntityId,
        origin: WorldPosition,
    ) {
        // An attack-move member between fights keeps its way back to its slot.
        let resume = self.combat.resuming.remove(&id).map(|r| r.slot);
        self.leave_group(id);
        self.begin_attack(id, target);
        if let Some(a) = self.combat.attackers.get_mut(&id) {
            a.origin = Some(origin);
            a.repath_tick = self.tick.0;
            a.resume = resume;
        }
    }

    pub(super) fn pursuit_allowed(
        &mut self,
        id: EntityId,
        goal: Goal,
        changed: &mut Vec<EntityId>,
    ) -> bool {
        let Some(origin) = self.combat.attackers.get(&id).and_then(|a| a.origin) else {
            return true;
        };
        let Some(u) = self.unit(id) else {
            return false;
        };
        let allowed = match u.stance {
            CombatStance::Aggressive => true,
            CombatStance::NoAttack => false,
            CombatStance::StandGround => self.unit_stats(id).is_some_and(|s| {
                goal.distance_squared(u.position) <= super::super::navy::attack_range(&s).pow(2)
            }),
            CombatStance::Defensive => self.unit_stats(id).is_some_and(|s| {
                goal.distance_squared(origin)
                    <= (5 * MELEE_RANGE_SUBUNITS + super::super::navy::attack_range(&s)).pow(2)
            }),
        };
        if !allowed {
            self.end_attack(id, changed);
            if u.stance == CombatStance::Defensive && u.position != origin {
                let _ = self.issue_task_move(id, origin);
            }
        }
        allowed
    }
}
