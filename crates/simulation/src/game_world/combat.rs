//! Authoritative Dark Age combat: hit points, armour, attack orders,
//! retaliation, death and building destruction.
//!
//! Work per tick is bounded by the number of units that currently hold an
//! attack order (`Combat::attackers`), never by the unit count: there is no
//! automatic target search and no all-pairs scan. A unit idles until it is
//! ordered to attack or is hit while idle (retaliation).
use super::{EconomyError, GameUnit, GameWorld, UnitTask};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, Tick};
use std::collections::{BTreeMap, BTreeSet};

mod approach;
#[path = "combat/castle/mod.rs"]
mod castle;
mod cost;
mod hash;
mod lifecycle;
pub(crate) mod military;
mod projectiles;
mod table;
mod telemetry;
pub(super) mod tick;
pub use telemetry::CombatTelemetry;
mod towers;

#[cfg(test)]
pub(crate) use approach::{FarAttack, MAX_FAR_SEARCHES_PER_TICK};
pub(crate) use hash::hash_hit;
pub use military::{apply_research_stats, researched_stats};
pub use projectiles::{Projectile, ProjectileKind};
pub(super) use table::age_armour;
pub use table::{
    BUILDING_COMBAT, BuildingAttack, BuildingCombat, DamageKind, UNIT_COMBAT, UnitCombat, UnitCost,
    UnitTraining, building_combat, damage_dealt, unit_combat,
};

/// A melee attacker reaches targets within one tile (centre to edge).
pub const MELEE_RANGE_SUBUNITS: i64 = FIXED_SUBUNITS_PER_TILE as i64;
/// A dead unit stays visible, at 0 HP, so a client can play a death animation.
pub const DYING_SECONDS: u32 = 1;
/// Corpse residency after the die animation. Shared by wire consumers.
pub const CORPSE_SECONDS: u32 = 30;
/// An approaching attacker re-plans its route at most this often.
pub const REPATH_SECONDS: u32 = 1;

/// The most recent damage a unit or building took.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HitRecord {
    pub attacker: EntityId,
    pub tick: Tick,
}

/// Attack and local healing work done by the most recent `advance`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombatStats {
    pub attackers: u32,
    pub hits: u32,
    /// Spatial bucket entries examined to prepare healing candidates this tick.
    pub healing_scanned_units: u64,
    /// Wounded candidate entries considered by healers this tick.
    pub healing_candidates: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Attacker {
    pub(crate) target: EntityId,
    pub(crate) repath_tick: u64,
    /// The firing tile of the last far search, while it can still be reused.
    pub(crate) far: Option<approach::FarAttack>,
}

#[derive(Debug, Default)]
pub(crate) struct Combat {
    pub(crate) healers: BTreeSet<EntityId>,
    pub(crate) projectiles: BTreeMap<u64, Projectile>,
    pub(crate) next_projectile: u64,
    pub(crate) tower_ready: BTreeMap<EntityId, u64>,
    pub(crate) attackers: BTreeMap<EntityId, Attacker>,
    /// `(tick at which the body is removed, unit)` in death order.
    pub(crate) dying: BTreeSet<(u64, EntityId)>,
    pub(crate) stats: CombatStats,
    telemetry: telemetry::Telemetry,
    /// Far firing-tile searches still allowed this tick.
    pub(crate) far_searches: u32,
}

impl Combat {
    pub(crate) fn is_idle(&self) -> bool {
        self.healers.is_empty()
            && self.attackers.is_empty()
            && self.dying.is_empty()
            && self.tower_ready.is_empty()
            && self.projectiles.is_empty()
    }
}

impl GameWorld {
    pub fn combat_stats(&self) -> CombatStats {
        self.combat.stats
    }

    /// Order `id` to attack another player's unit or building. The unit walks
    /// into attack range, then hits every attack period until the target is
    /// gone. A plain move or gather order cancels the attack.
    pub fn issue_attack(
        &mut self,
        player: aoe_core::PlayerId,
        id: EntityId,
        target: EntityId,
    ) -> Result<(), EconomyError> {
        let unit = self
            .unit(id)
            .filter(|unit| unit.hp > 0 && !self.embarked(unit.id))
            .ok_or(EconomyError::UnknownEntity)?;
        if unit.player != player {
            return Err(EconomyError::NotOwner);
        }
        if unit_combat(unit.kind).attack == 0 {
            return Err(EconomyError::WrongUnitKind);
        }
        let goal = self
            .attack_goal(target, player)
            .filter(|_| {
                id != target
                    && (unit_combat(unit.kind).attack_kind != DamageKind::Pierce
                        || self.target_visible_to(player, target))
            })
            .ok_or(EconomyError::InvalidTarget)?;
        let Some(&index) = self.lookup.get(&id) else {
            return Err(EconomyError::UnknownEntity);
        };
        let range =
            super::navy::attack_range(&self.unit_stats(id).ok_or(EconomyError::UnknownEntity)?);
        if goal.distance_squared(unit.position) <= range * range {
            self.stop_movement(index);
        } else if let Some(destination) = self.attack_destination(id, &goal)? {
            self.issue_task_move(id, destination)?;
        } else {
            return Err(EconomyError::NoAttackPosition);
        }
        self.stop_gathering(id);
        self.economy.builders.remove(&id);
        self.cancel_transport_journey(id);
        self.begin_attack(id, target);
        Ok(())
    }

    /// Sets the attack target and the visible state. The hit rate belongs to
    /// the unit (`StoredUnit::next_hit_tick`), so no order sequence can speed it up.
    pub(crate) fn begin_attack(&mut self, id: EntityId, target: EntityId) {
        self.cancel_auto_scout(id);
        let Some(&index) = self.lookup.get(&id) else {
            return;
        };
        self.units[index].last_movement_error = None;
        self.combat.attackers.insert(
            id,
            Attacker {
                target,
                repath_tick: self.tick.0.saturating_add(self.repath_ticks()),
                far: None,
            },
        );
        let state = &mut self.units[index].state;
        state.attack_target = Some(target);
        state.task = UnitTask::Attacking;
    }

    /// A move or gather order replaces any attack order.
    pub(crate) fn cancel_attack(&mut self, id: EntityId) {
        if self.combat.attackers.remove(&id).is_some()
            && let Some(&index) = self.lookup.get(&id)
        {
            self.units[index].state.attack_target = None;
        }
    }

    pub(crate) fn repath_ticks(&self) -> u64 {
        u64::from(self.config.tick_hz) * u64::from(REPATH_SECONDS)
    }

    pub(crate) fn attack_period_ticks(&self, unit: &GameUnit) -> u32 {
        let centiseconds = unit_combat(unit.kind).attack_period_centiseconds;
        (self.config.tick_hz.saturating_mul(centiseconds) / 100).max(1)
    }
}
