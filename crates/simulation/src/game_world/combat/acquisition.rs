//! Idle acquisition and stance control; controller ownership is irrelevant.
use super::{
    MELEE_RANGE_SUBUNITS,
    targeting::{BuildingTargets, TargetArea},
    tick::Goal,
};
use crate::{EconomyError, GameWorld, UnitKind, UnitTask};
use aoe_core::{CombatStance, EntityId, PlayerId, WorldPosition};
use std::ops::Bound::{Excluded, Unbounded};
pub(crate) mod sleep;

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
                && !(self.tick.0 + u64::from(id.0)).is_multiple_of(self.config.ticks(4)))
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
            sight: range,
            actor: id,
            ram: u.kind.is_ram(),
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
            self.combat.quiet.wake(id);
            self.vassal_player_override(id);
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
        #[cfg(test)]
        self.assert_quiet_acquisition();
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
            if self.skip_quiet_acquisition(id) {
                continue;
            }
            self.combat.stats.acquisition_checks += 1;
            let Some(u) = self.unit(id) else {
                continue;
            };
            if (u.kind.is_naval() || u.kind == UnitKind::Villager)
                && self.units[self.lookup[&id]].order.is_none()
            {
                self.clear_scalar_march(id);
                if u.kind == UnitKind::Villager {
                    continue;
                }
            }
            if u.hp > 0 && (u.auto_scout || u.stance == CombatStance::NoAttack || self.embarked(id))
            {
                if !self.economy.no_sleep {
                    self.combat.quiet.park(id);
                }
                continue;
            }
            let building_attack = self
                .combat
                .attackers
                .get(&id)
                .copied()
                .filter(|a| a.origin.is_some())
                .and_then(|a| {
                    self.building(a.target)
                        .map(|b| (a, b.kind.is_obstacle_target()))
                });
            // Offensive movement is ordinary movement plus aggression: a
            // marching attack-move member — a lone unit, a formation member or
            // a General's soldier — strikes what is already in its weapon's
            // reach instead of walking past it. The group's bounded engage
            // look still decides who leaves the march to chase.
            let marching = self.offensive_continuation(id).is_some()
                && (u.moving || u.planning || matches!(u.task, UnitTask::Moving));
            if u.hp == 0
                || ((u.moving || u.planning) && building_attack.is_none() && !marching)
                || (!matches!(u.task, UnitTask::Idle) && building_attack.is_none() && !marching)
            {
                continue;
            }
            // A unit busy on a wall, gate or road looks for a better target
            // once every few ticks, spread over units like the march scans.
            if matches!(building_attack, Some((_, true)))
                && !(self.tick.0 + u64::from(id.0)).is_multiple_of(self.config.ticks(4))
            {
                continue;
            }
            let Some(mut area) = self.acquisition_area(&u) else {
                continue;
            };
            if marching {
                // Marching members stop for an enemy in attack range only.
                let range = super::super::navy::attack_range(&self.unit_state_stats(&u));
                area.max = area.max.min(range);
            }
            let buildings = match building_attack {
                Some((_, true)) => BuildingTargets::RecheckObstacle,
                Some((_, false)) => BuildingTargets::None,
                // A marcher can leave to pursue an isolated obstacle. Spread
                // those wider looks out like the formation's periodic scans.
                // A grouped soldier leaves wall work to its group's breach: its
                // own look would pull it out of the formation and lose the
                // order the wall stands in the way of.
                None if self.in_breaching_group(id) => BuildingTargets::Ordinary,
                None if marching
                    && u.stance != CombatStance::StandGround
                    && (self.tick.0 + u64::from(id.0)).is_multiple_of(self.config.ticks(4)) =>
                {
                    BuildingTargets::Marching
                }
                None => BuildingTargets::Unit,
            };
            // A melee unit does not take on an enemy whose every spot is
            // taken: it would only stand and wait out of reach.
            let probe = self.acquire_target(&area, buildings);
            let target = probe.target;
            if target.is_none()
                && !probe.sampled_hostile
                && matches!(u.task, UnitTask::Idle)
                && !u.moving
                && !u.planning
            {
                self.sleep_quiet_acquisition(&u, &area);
            }
            if buildings == BuildingTargets::RecheckObstacle && probe.preferred_in_sight {
                self.end_attack(id, changed);
            }
            // A recheck that finds the obstacle already under attack keeps that
            // attack: beginning it again would drop the way back to the march.
            let target = target.filter(|&t| building_attack.is_none_or(|(a, _)| a.target != t));
            if let Some(target) = target.filter(|&target| self.attack_spot_free(id, target)) {
                if matches!(building_attack, Some((_, false))) {
                    self.stop_movement(self.lookup[&id]);
                }
                self.begin_autonomous_attack(
                    id,
                    target,
                    building_attack
                        .and_then(|(attacker, _)| attacker.origin)
                        .unwrap_or(u.position),
                );
                changed.push(id);
            }
        }
    }

    /// A fighter whose target is gone looks for the next enemy at once, within
    /// a reserved part of the scan budget, instead of waiting a turn among
    /// every idle unit of the world for the rotating acquisition pass.
    pub(super) fn next_target_after_kill(
        &mut self,
        id: EntityId,
        origin: WorldPosition,
        changed: &mut Vec<EntityId>,
    ) {
        if self.combat.stats.targeting_scanned >= super::targeting::MAX_TARGET_INSPECTIONS * 3 / 4
            || !autonomous_kind(self.unit(id).map_or(UnitKind::Villager, |u| u.kind))
        {
            return;
        }
        let Some(u) = self.unit(id).filter(|u| {
            u.hp > 0
                && !u.auto_scout
                && u.stance != CombatStance::NoAttack
                && matches!(u.task, UnitTask::Idle)
                && !self.embarked(u.id)
        }) else {
            return;
        };
        let Some(area) = self.acquisition_area(&u) else {
            return;
        };
        if let Some(target) = self
            .acquire_target(&area, BuildingTargets::None)
            .target
            .filter(|&target| self.attack_spot_free(id, target))
        {
            self.begin_autonomous_attack(id, target, origin);
            changed.push(id);
        }
    }

    /// The sight and range window within which an idle unit looks for enemies.
    pub(super) fn acquisition_area(&self, u: &crate::GameUnit) -> Option<TargetArea> {
        let (_, _, los) = self.unit_observer(u)?;
        let stats = self.unit_state_stats(u);
        let range = super::super::navy::attack_range(&stats);
        let sight = i64::from(los) * MELEE_RANGE_SUBUNITS;
        let max = if u.stance == CombatStance::StandGround {
            range.min(sight)
        } else {
            sight
        };
        Some(TargetArea {
            player: u.player,
            source: Goal::Unit(u.position, u.kind.collision_radius()),
            min: i64::from(stats.min_range) * MELEE_RANGE_SUBUNITS,
            max,
            sight,
            actor: u.id,
            ram: u.kind.is_ram(),
        })
    }

    /// One bounded look for an enemy, as an idle unit would make.
    pub(crate) fn probe_for_target(&mut self, id: EntityId) -> Option<EntityId> {
        let u = self.unit(id)?;
        if u.stance == CombatStance::NoAttack || u.auto_scout {
            return None;
        }
        let area = self.acquisition_area(&u)?;
        self.acquire_target(&area, BuildingTargets::Unit).target
    }

    pub(crate) fn begin_autonomous_attack(
        &mut self,
        id: EntityId,
        target: EntityId,
        origin: WorldPosition,
    ) {
        // An attack-move member between fights keeps its way back to its slot.
        let continuation = self.offensive_continuation(id);
        let queue = continuation.map(|_| self.detached_waypoints(id));
        let resume = self.combat.resuming.remove(&id).or(continuation);
        self.leave_group(id);
        self.begin_attack(id, target);
        if let Some(a) = self.combat.attackers.get_mut(&id) {
            a.origin = Some(origin);
            a.repath_tick = self.tick.0;
            a.resume = resume;
        }
        if let Some(queue) = queue.filter(|queue| !queue.pending.is_empty()) {
            self.move_queues.insert(id, queue);
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
            if u.stance == CombatStance::Defensive
                && u.position != origin
                && !self.combat.resuming.contains_key(&id)
            {
                let _ = self.issue_task_move(id, origin);
            }
        }
        allowed
    }
}
