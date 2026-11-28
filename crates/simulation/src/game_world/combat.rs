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
mod conversion;
mod cost;
#[path = "combat/damage/mod.rs"]
mod damage;
mod hash;
mod lifecycle;
pub(crate) mod military;
mod projectiles;
mod retaliation;
#[path = "combat/siege/mod.rs"]
mod siege;
pub use siege::{
    TREBUCHET_TRANSFORM_WORK, TREBUCHET_WORK_PER_SECOND_TENTHS, trebuchet_transform_ticks,
};
mod spots;
mod table;
mod targeting;
mod telemetry;
pub(super) mod tick;
pub use telemetry::{CombatTelemetry, DamageObservation};
mod towers;

#[cfg(test)]
pub(crate) use approach::FarAttack;
pub(crate) use approach::SearchWait;
pub use classes::unit_building_bonus;
pub use conversion::{conversion_chance, convertible_building, convertible_unit};
pub(crate) use hash::hash_hit;
pub use military::{apply_research_stats, class_bonus, researched_stats};
pub use projectiles::{DatProjectile, Projectile, ProjectileKind, dat_projectile};
pub use siege::native_attack_windup_millis;
pub use table::{
    BUILDING_COMBAT, BuildingAttack, BuildingCombat, DamageKind, FISHING_SHIP_SPEED_HUNDREDTHS,
    UNIT_COMBAT, UnitCombat, UnitCost, UnitTraining, building_combat, damage_dealt, unit_combat,
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// Inactive or empty-region turns replayed without visiting a unit.
    pub acquisition_slept: u32,
    /// Bucket entries inspected by military and defensive buildings together.
    pub targeting_scanned: u32,
    /// Pursuit searches in the combat phase (order validation is separate).
    pub attack_searches: u32,
    /// First blows struck this tick after a target came into reach, and the
    /// ticks each waited since (the delay from contact to the first hit).
    pub first_hits: u32,
    pub first_hit_wait_ticks: u64,
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
    pub(crate) resume: Option<super::formation::Resume>,
    /// Start tick of this native attack cycle, or zero before its wind-up.
    pub(crate) swing_started: u64,
    /// The tick its target came into reach, until its first blow there.
    /// Diagnostics only (`CombatStats::first_hits`): never read by play,
    /// saved or hashed.
    pub(crate) in_reach_since: Option<u64>,
}

#[derive(Debug, Default)]
pub(crate) struct Combat {
    /// Fraction above displayed HP, indexed by allocated identity. Shelter and
    /// transport preserve identities; allocation happens at spawn, never a hit.
    pub(crate) hp_quarters: Vec<u8>,
    pub(crate) seekers: BTreeSet<EntityId>,
    pub(crate) quiet: acquisition::sleep::Quiet,
    pub(crate) tower_cursor: EntityId,
    pub(crate) tower_targets: BTreeMap<EntityId, EntityId>,
    pub(crate) target_rounds: BTreeMap<EntityId, u64>,
    /// Enemies per chunk and viewer, shared by the looks of one combat step.
    pub(crate) contacts: targeting::contacts::ContactCache,
    pub(crate) healers: BTreeSet<EntityId>,
    pub(crate) healing: castle::Rest,
    pub(crate) projectiles: BTreeMap<u64, Projectile>,
    /// `projectiles` by impact tick, so a tick reads only those that land.
    pub(crate) impacts: BTreeSet<(Tick, u64)>,
    pub(crate) next_projectile: u64,
    pub(crate) tower_ready: BTreeMap<EntityId, u64>,
    pub(crate) attackers: BTreeMap<EntityId, Attacker>,
    /// The attackers' targets, sorted, taken as each movement step starts:
    /// a unit being chased keeps the full model like its pursuers.
    pub(crate) chased: Vec<EntityId>,
    /// End ticks of stationary transitions, keyed by unit in its destination form.
    pub(crate) trebuchets_transforming: BTreeMap<EntityId, u64>,
    /// Units on attack-move legs that use individual routes.
    pub(crate) scalar_marches: BTreeSet<EntityId>,
    pub(crate) patrols: BTreeMap<EntityId, [aoe_core::WorldPosition; 2]>,
    /// Attack-move members between fights, waiting to march on to their slots.
    pub(crate) resuming: BTreeMap<EntityId, super::formation::Resume>,
    /// `(tick at which the body is removed, unit)` in death order.
    pub(crate) dying: BTreeSet<(u64, EntityId)>,
    /// Sunk Fire Ships of players with Incendiaries, blasting at the end of the
    /// combat step: `(ship, owner, position)`.
    pub(crate) blasts: Vec<(EntityId, aoe_core::PlayerId, aoe_core::WorldPosition)>,
    pub(crate) stats: CombatStats,
    telemetry: telemetry::Telemetry,
    /// Searches the attacker phase computed side by side on its starting
    /// state, consumed in attacker order and always empty between ticks:
    /// their goal and the outcome.
    pub(crate) prefetched: BTreeMap<EntityId, (tick::Goal, approach::AttackSearch)>,
    /// Approaches deferred by a crowd since the world began.
    #[cfg(test)]
    pub(crate) waits: u64,
    /// Prefetched searches the attacker loop used since the world began.
    #[cfg(test)]
    pub(crate) prefetch_used: u64,
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
        self.issue_attack_searching(player, id, target, true)
    }

    /// `issue_attack` for one member of a larger order or a restored save:
    /// the firing-position search runs in this tick's combat phase, where the
    /// searches of a burst are computed side by side (`approach/prefetch.rs`)
    /// instead of one after another inside the command (#978).
    pub(crate) fn issue_attack_in_combat_phase(
        &mut self,
        player: aoe_core::PlayerId,
        id: EntityId,
        target: EntityId,
    ) -> Result<(), EconomyError> {
        self.issue_attack_searching(player, id, target, false)
    }

    fn issue_attack_searching(
        &mut self,
        player: aoe_core::PlayerId,
        id: EntityId,
        target: EntityId,
        search_now: bool,
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
        if unit.holds_relic
            || (unit_combat(unit.kind).attack == 0 && unit.kind != crate::UnitKind::Monk)
        {
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
                if a.resume.take().is_some() {
                    self.move_queues.remove(&id);
                }
            }
            self.vassal_player_override(id);
            return Ok(());
        }
        let range =
            super::navy::attack_range(&self.unit_stats(id).ok_or(EconomyError::UnknownEntity)?);
        let replaces_offensive = self.offensive_resume(id).is_some();
        let mut initial_far = None;
        if goal.distance_squared(unit.position) <= range * range
            && (!Self::uses_melee_position(unit.kind) || self.arrival_position_free(index))
        {
            self.stop_movement(index);
        } else if !search_now {
            self.stop_unit_orders(index);
            if replaces_offensive {
                self.move_queues.remove(&id);
            }
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
                        // Crowding defers the approach; it is not proof
                        // that the target is unreachable.
                        self.stop_movement(index);
                    }
                }
                result => {
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
        if replaces_offensive {
            self.move_queues.remove(&id);
        }
        self.begin_attack(id, target);
        if let Some(entry) = self.combat.attackers.get_mut(&id) {
            entry.far = initial_far;
        }
        Ok(())
    }

    /// Sets the attack target and the visible state. The hit rate belongs to
    /// the unit (`StoredUnit::next_hit_tick`), so no order sequence can speed it up.
    pub(crate) fn begin_attack(&mut self, id: EntityId, target: EntityId) {
        self.clear_scalar_march(id);
        self.combat.quiet.wake(id);
        self.combat.healing.wake(id);
        self.cancel_auto_scout(id);
        self.combat.resuming.remove(&id);
        let Some(&index) = self.lookup.get(&id) else {
            return;
        };
        if self.units[index].state.kind == crate::UnitKind::Monk {
            self.cancel_relic_order(id);
        }
        self.set_movement_error(index, None);
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
                swing_started: 0,
                in_reach_since: None,
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
        self.clear_scalar_march(id);
        self.cancel_patrol(id);
        self.combat.resuming.remove(&id);
        if self.combat.attackers.remove(&id).is_some()
            && let Some(&index) = self.lookup.get(&id)
        {
            self.units[index].state.attack_target = None;
        }
    }

    pub(crate) fn clear_scalar_march(&mut self, id: EntityId) {
        if self.combat.scalar_marches.remove(&id)
            && self
                .unit(id)
                .is_some_and(|u| u.kind == crate::UnitKind::Villager)
        {
            self.combat.seekers.remove(&id);
            self.combat.quiet.leave(id);
        }
    }

    pub(crate) fn repath_ticks(&self) -> u64 {
        u64::from(self.config.tick_hz) * u64::from(REPATH_SECONDS)
    }

    pub(crate) fn attack_period_ticks(&self, unit: &GameUnit) -> u32 {
        let mut centiseconds = unit_combat(unit.kind).attack_period_centiseconds;
        let economy = self.economy.players.get(&unit.player);
        let researched = economy.map_or(0, |p| p.researched);
        if researched & crate::Tech::ThumbRing.bit() != 0
            && (unit.kind.is_archery() || unit.kind.is_mounted_archer())
        {
            centiseconds = centiseconds * 100 / 115;
        }
        (self.config.tick_hz.saturating_mul(centiseconds) / 100).max(1)
    }

    // Military deadlines use half-tick units. Deducting one half-tick for
    // every starved tick preserves progress when a player's supply changes.
    fn weapon_clock(&self, unit: &GameUnit) -> (u64, u64) {
        let now = self.tick.0;
        if !unit.kind.needs_food_upkeep() {
            return (now, 1);
        }
        let state = self.economy.players.get(&unit.player);
        let slow_ticks = state.map_or(0, |p| p.upkeep_slow_ticks);
        let shortage = state.is_some_and(|p| p.upkeep_shortage && p.stockpile.food == 0);
        (
            now.saturating_mul(2).saturating_sub(slow_ticks),
            2 - u64::from(shortage),
        )
    }

    pub(crate) fn set_weapon_cooldown(&mut self, index: usize, ticks: u64) {
        let unit = &self.units[index].state;
        let (clock, _) = self.weapon_clock(unit);
        let scale = 1 + u64::from(unit.kind.needs_food_upkeep());
        self.units[index].next_hit_tick = clock.saturating_add(ticks.saturating_mul(scale));
    }

    /// Project the cooldown deadline onto world ticks at the current speed.
    pub(crate) fn weapon_ready_tick(&self, index: usize) -> u64 {
        let unit = &self.units[index];
        if unit.next_hit_tick == 0 {
            return 0;
        }
        let (clock, speed) = self.weapon_clock(&unit.state);
        if let Some(remaining) = unit.next_hit_tick.checked_sub(clock) {
            self.tick.0.saturating_add(remaining.div_ceil(speed))
        } else {
            self.tick
                .0
                .saturating_sub((clock - unit.next_hit_tick) / speed)
        }
    }
}
