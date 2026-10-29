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
    /// Autonomous melee fights the enemy already in reach before pursuing a
    /// stale choice through the front. Explicit attack orders keep their target.
    pub(super) fn refresh_contact_target(
        &mut self,
        index: usize,
        mut attacker: super::Attacker,
        goal: Option<super::tick::Goal>,
        stats: &super::UnitCombat,
        changed: &mut Vec<EntityId>,
    ) -> super::Attacker {
        let u = self.units[index].state;
        let id = u.id;
        let ready = self.tick.0 >= self.units[index].next_hit_tick;
        if (attacker.origin.is_none() && attacker.resume.is_none())
            || !ready
            // First contact reacts immediately; repeat probes keep the bounded
            // four-tick acquisition cadence after a fighter has struck.
            || (self.units[index].next_hit_tick != 0
                && !(self.tick.0 + u64::from(id.0)).is_multiple_of(4))
            || self.combat.stats.targeting_scanned >= super::targeting::MAX_TARGET_INSPECTIONS / 2
            || u.kind.is_naval()
            || !Self::uses_melee_position(u.kind)
        {
            return attacker;
        }
        let range = super::super::navy::attack_range(stats);
        let min = i64::from(stats.min_range) * MELEE_RANGE_SUBUNITS;
        if goal.is_some_and(|g| {
            let distance = g.distance_squared(u.position);
            distance >= min * min && distance <= range * range
        }) {
            return attacker;
        }
        if self.unit_observer(&u).is_none() {
            return attacker;
        }
        let area = TargetArea {
            player: u.player,
            source: super::tick::Goal::Unit(u.position, u.kind.collision_radius()),
            min,
            max: range,
            actor: id,
            ram: u.kind == UnitKind::BatteringRam,
        };
        if let Some(target) = self.acquire_contact_target(&area) {
            attacker.target = target;
            attacker.repath_tick = self.tick.0;
            attacker.far = None;
            attacker.goal_tile = None;
            self.combat.attackers.insert(id, attacker);
            self.units[index].state.attack_target = Some(target);
            changed.push(id);
        }
        attacker
    }

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
            // A melee unit does not take on an enemy whose every spot is
            // taken: it would only stand and wait out of reach.
            if let Some(target) = self
                .acquire_target(&area, building_attack.is_none())
                .filter(|&target| self.attack_spot_free(id, target))
            {
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
        let stats = self.unit_state_stats(u);
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
