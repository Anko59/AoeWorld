//! Authoritative Dark Age combat: hit points, armour, attack orders,
//! retaliation, death and building destruction.
//!
//! Target searches rotate through a bounded active index and spatial buckets;
//! pursuit searches share one tick budget with explicit attack orders.
use super::{EconomyError, GameUnit, GameWorld, UnitTask};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, Tick};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) mod acquisition;
mod approach;
#[path = "combat/castle/mod.rs"]
mod castle;
#[path = "combat/classes/mod.rs"]
mod classes;
mod cost;
mod hash;
mod lifecycle;
pub(crate) mod military;
mod projectiles;
mod retaliation;
mod spots;
mod table;
mod targeting;
mod telemetry;
pub(super) mod tick;
pub use telemetry::CombatTelemetry;
mod towers;

#[cfg(test)]
pub(crate) use approach::FarAttack;
pub(crate) use approach::{MAX_POSITION_SEARCHES_PER_TICK, SearchWait};
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
    /// Military identities visited by the fair acquisition rotation.
    pub acquisition_checks: u32,
    /// Bucket entries inspected by military and defensive buildings together.
    pub targeting_scanned: u32,
    /// Pursuit searches in the combat phase (order validation is separate).
    pub attack_searches: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Attacker {
    pub(crate) target: EntityId,
    pub(crate) repath_tick: u64,
    /// The firing tile of the last far search, while it can still be reused.
    pub(crate) far: Option<approach::FarAttack>,
    pub(crate) goal_tile: Option<aoe_core::TileCoord>,
    /// Origin for autonomous pursuit; explicit orders override the stance.
    pub(crate) origin: Option<aoe_core::WorldPosition>,
    /// An attack-move member's slot at the group destination. When the fight
    /// ends and nothing else is in reach, the member marches there again.
    pub(crate) resume: Option<aoe_core::WorldPosition>,
}

#[derive(Debug)]
pub(crate) struct Combat {
    pub(crate) seekers: BTreeSet<EntityId>,
    pub(crate) seeker_cursor: EntityId,
    pub(crate) tower_cursor: EntityId,
    pub(crate) tower_targets: BTreeMap<EntityId, EntityId>,
    pub(crate) target_rounds: BTreeMap<EntityId, u64>,
    pub(crate) search_tick: u64,
    pub(crate) searches_used: u32,
    pub(crate) pursuit_cursor: Option<EntityId>,
    /// Current phase's admission floor; cleared before returning to callers.
    pub(crate) pursuit_floor: Option<EntityId>,
    pub(crate) healers: BTreeSet<EntityId>,
    pub(crate) projectiles: BTreeMap<u64, Projectile>,
    pub(crate) next_projectile: u64,
    pub(crate) tower_ready: BTreeMap<EntityId, u64>,
    pub(crate) attackers: BTreeMap<EntityId, Attacker>,
    /// Attack-move members between fights, waiting to march on to their slots.
    pub(crate) resuming: BTreeMap<EntityId, super::formation::Resume>,
    /// `(tick at which the body is removed, unit)` in death order.
    pub(crate) dying: BTreeSet<(u64, EntityId)>,
    pub(crate) stats: CombatStats,
    telemetry: telemetry::Telemetry,
    /// Expensive firing-position searches still allowed this tick.
    pub(crate) position_searches: u32,
    /// Deferred approaches since the world began: (crowded, budget).
    #[cfg(test)]
    pub(crate) waits: (u64, u64),
}

impl Default for Combat {
    fn default() -> Self {
        Self {
            seekers: Default::default(),
            seeker_cursor: Default::default(),
            tower_cursor: Default::default(),
            tower_targets: Default::default(),
            target_rounds: Default::default(),
            search_tick: 0,
            searches_used: 0,
            pursuit_cursor: None,
            pursuit_floor: None,
            healers: Default::default(),
            projectiles: Default::default(),
            next_projectile: 0,
            tower_ready: Default::default(),
            attackers: Default::default(),
            resuming: Default::default(),
            dying: Default::default(),
            stats: Default::default(),
            telemetry: Default::default(),
            position_searches: MAX_POSITION_SEARCHES_PER_TICK,
            #[cfg(test)]
            waits: (0, 0),
        }
    }
}

impl Combat {
    pub(crate) fn is_idle(&self) -> bool {
        self.healers.is_empty()
            && self.seekers.is_empty()
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

    /// Order an owned unit or defensive building to attack an enemy. Units
    /// pursue; buildings require a target already in range. Weapon cooldowns
    /// survive replacement orders. A move or gather cancels a unit's attack.
    pub fn issue_attack(
        &mut self,
        player: aoe_core::PlayerId,
        id: EntityId,
        target: EntityId,
    ) -> Result<(), EconomyError> {
        if self.building(id).is_some() {
            return self.issue_building_attack(player, id, target);
        }
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
        // Repeating an accepted target does not restart a route or spend a
        // second firing-position search (nor reset the weapon cooldown).
        if self
            .combat
            .attackers
            .get(&id)
            .is_some_and(|a| a.target == target)
        {
            if let Some(a) = self.combat.attackers.get_mut(&id) {
                a.origin = None;
            }
            return Ok(());
        }
        let range =
            super::navy::attack_range(&self.unit_stats(id).ok_or(EconomyError::UnknownEntity)?);
        let search_before = (
            self.combat.search_tick,
            self.combat.searches_used,
            self.combat.stats.attack_searches,
        );
        let mut initial_far = None;
        let mut retry_next_tick = false;
        if goal.distance_squared(unit.position) <= range * range
            && (!Self::uses_melee_position(unit.kind) || self.arrival_position_free(index))
        {
            self.stop_movement(index);
        } else if !self.reserve_attack_search() {
            self.stop_unit_orders(index);
            self.begin_attack(id, target);
            if let Some(attacker) = self.combat.attackers.get_mut(&id) {
                attacker.repath_tick = self.tick.0;
            }
            return Ok(());
        } else {
            match self.attack_search(id, &goal) {
                Ok(search) if search.destination.is_some() || search.wait.is_some() => {
                    if let Some(destination) = search.destination {
                        self.issue_task_move(id, destination)?;
                        if search.far {
                            initial_far = Some(approach::FarAttack {
                                centre: goal.centre().tile_floor(),
                                destination,
                            });
                        }
                    } else {
                        // Crowding or a spent position budget defers the
                        // approach; it is not proof that the target is unreachable.
                        self.stop_movement(index);
                        retry_next_tick = search.wait == Some(SearchWait::Budget);
                    }
                }
                result => {
                    // Rejected validation leaves the pursuit admission intact.
                    // The expensive position search it ran still spends the
                    // shared per-tick budget, so rejected orders stay bounded.
                    (
                        self.combat.search_tick,
                        self.combat.searches_used,
                        self.combat.stats.attack_searches,
                    ) = search_before;
                    return match result {
                        Err(error) => Err(error.into()),
                        _ => Err(EconomyError::NoAttackPosition),
                    };
                }
            }
        }
        self.stop_gathering(id);
        self.economy.builders.remove(&id);
        self.cancel_transport_journey(id);
        self.begin_attack(id, target);
        if let Some(entry) = self.combat.attackers.get_mut(&id) {
            entry.far = initial_far;
            if retry_next_tick {
                entry.repath_tick = self.tick.0.saturating_add(1);
            }
        }
        Ok(())
    }

    /// Sets the attack target and the visible state. The hit rate belongs to
    /// the unit (`StoredUnit::next_hit_tick`), so no order sequence can speed it up.
    pub(crate) fn begin_attack(&mut self, id: EntityId, target: EntityId) {
        self.cancel_auto_scout(id);
        self.combat.resuming.remove(&id);
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
                goal_tile: self
                    .attack_goal(target, self.units[index].state.player)
                    .map(|g| g.centre().tile_floor()),
                origin: None,
                resume: None,
            },
        );
        let state = &mut self.units[index].state;
        state.attack_target = Some(target);
        state.task = if state.planning {
            UnitTask::Moving
        } else {
            UnitTask::Attacking
        };
    }

    pub(super) fn mark_attack_approach(&mut self, index: usize) {
        let state = &mut self.units[index].state;
        state.task = if state.moving {
            UnitTask::Attacking
        } else {
            UnitTask::Moving
        };
    }

    /// A move, gather or stop order replaces any attack order, and with it an
    /// attack-move member's pending way back to its slot.
    pub(crate) fn cancel_attack(&mut self, id: EntityId) {
        self.combat.resuming.remove(&id);
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
