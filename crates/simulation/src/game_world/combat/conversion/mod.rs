//! Monk conversion, the smallest correct version of DE's rules. A Monk given an
//! attack order on an enemy tries to convert it each time its weapon is ready;
//! the Monastery technologies shape the outcome (see
//! docs/economy/generic-techs.md for what this leaves out: no automatic
//! conversion, no conversion delay by distance, no adjacency rule for
//! buildings and rams).
use crate::{BuildingKind, GameWorld, Tech, TechMask, UnitKind};
use aoe_core::{EntityId, PlayerId};

/// Faith drained by a successful conversion: seconds before the Monk can try
/// again (Illumination halves it).
const REST_SECONDS: u64 = 62;
/// Base chance, in thousandths, that an attempt succeeds. Devotion makes units
/// 15% and Faith 50% harder to convert.
const FULL_CHANCE: u64 = 1000;

/// Buildings a Redemption Monk may convert: everything but Town Centers,
/// Castles, Monasteries, Farms, walls, gates, bridges and roads.
pub const fn convertible_building(kind: BuildingKind) -> bool {
    matches!(
        kind,
        BuildingKind::House
            | BuildingKind::LumberCamp
            | BuildingKind::MiningCamp
            | BuildingKind::Mill
            | BuildingKind::Barracks
            | BuildingKind::Outpost
            | BuildingKind::Dock
            | BuildingKind::ArcheryRange
            | BuildingKind::Stable
            | BuildingKind::Blacksmith
            | BuildingKind::Market
            | BuildingKind::WatchTower
            | BuildingKind::University
            | BuildingKind::SiegeWorkshop
    )
}

/// Whether a Monk of a player with `researched` may convert a unit of `kind`.
pub fn convertible_unit(kind: UnitKind, researched: TechMask) -> bool {
    let has = |t: Tech| researched & t.bit() != 0;
    match kind {
        UnitKind::Vassal | UnitKind::Steward | UnitKind::General => false,
        UnitKind::Monk => has(Tech::Atonement),
        k if k.is_siege() => has(Tech::Redemption),
        _ => true,
    }
}

/// Chance in thousandths that one attempt on a unit owned by a player with
/// `victim` technologies succeeds.
pub fn conversion_chance(victim: TechMask) -> u64 {
    if victim & Tech::Faith.bit() != 0 {
        FULL_CHANCE / 2
    } else if victim & Tech::Devotion.bit() != 0 {
        FULL_CHANCE * 85 / 100
    } else {
        FULL_CHANCE
    }
}

/// Splitmix64: one deterministic roll per (tick, monk, target).
fn roll(tick: u64, monk: EntityId, target: EntityId) -> u64 {
    let mut z = tick
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(u64::from(monk.0) << 32 | u64::from(target.0))
        .wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl GameWorld {
    /// One conversion attempt by `monk` on `target`.
    pub(super) fn monk_strike(
        &mut self,
        monk: EntityId,
        target: EntityId,
        changed: &mut Vec<EntityId>,
    ) {
        let Some(owner) = self.unit(monk).map(|u| u.player) else {
            return;
        };
        let mine = self.player_economy(owner).map_or(0, |p| p.researched);
        let (victim_player, is_unit) = if let Some(u) = self.unit(target) {
            if !convertible_unit(u.kind, mine) {
                self.end_attack(monk, changed);
                return;
            }
            (u.player, true)
        } else if let Some(b) = self
            .building(target)
            .map(|b| (b.kind, b.player, b.is_complete()))
        {
            if mine & Tech::Redemption.bit() == 0 || !convertible_building(b.0) || !b.2 {
                self.end_attack(monk, changed);
                return;
            }
            (b.1, false)
        } else {
            self.end_attack(monk, changed);
            return;
        };
        let theirs = self
            .player_economy(victim_player)
            .map_or(0, |p| p.researched);
        if roll(self.tick.0, monk, target) % FULL_CHANCE >= conversion_chance(theirs) {
            return;
        }
        // The attempt succeeded: the Monk (and, without Theocracy, every Monk
        // converting the same target) must recover its faith.
        let mut rest = REST_SECONDS * u64::from(self.config.tick_hz);
        if mine & Tech::Illumination.bit() != 0 {
            rest /= 2;
        }
        let resting: Vec<EntityId> = if mine & Tech::Theocracy.bit() != 0 {
            vec![monk]
        } else {
            self.combat
                .attackers
                .iter()
                .filter(|(id, a)| {
                    a.target == target
                        && self
                            .unit(**id)
                            .is_some_and(|u| u.kind == UnitKind::Monk && u.player == owner)
                })
                .map(|(id, _)| *id)
                .chain(std::iter::once(monk))
                .collect()
        };
        let until = self.tick.0 + rest;
        for id in resting {
            if let Some(&index) = self.lookup.get(&id) {
                self.units[index].next_hit_tick = until;
            }
        }
        if !is_unit {
            self.convert_building(target, owner);
        } else if theirs & Tech::Heresy.bit() != 0 {
            // Heresy: the unit dies instead of changing sides.
            if let Some(&index) = self.lookup.get(&target) {
                self.kill_unit(index);
                changed.push(target);
            }
        } else {
            self.convert_unit(target, owner, changed);
        }
    }

    /// Hands a unit to `to`: a new unit of the same kind, position, facing and
    /// hit points under the new owner replaces it (a new identity), so no
    /// order, job or formation of the old owner survives.
    pub(crate) fn convert_unit(&mut self, id: EntityId, to: PlayerId, changed: &mut Vec<EntityId>) {
        let Some(old) = self.unit(id).filter(|u| u.hp > 0) else {
            return;
        };
        let Ok(new) = self.spawn_kind(to, old.kind, old.position) else {
            return;
        };
        if let Some(economy) = self.economy.players.get_mut(&old.player) {
            economy.population_used = economy.population_used.saturating_sub(1);
        }
        self.wake_player_training(old.player);
        self.remove_unit(id);
        if let Some(&index) = self.lookup.get(&new) {
            let state = &mut self.units[index].state;
            state.hp = old.hp.min(state.max_hp);
            state.facing = old.facing;
            state.previous_position = old.position;
        }
        changed.push(new);
        self.update_unit_vision(new);
    }

    /// Hands a finished building to `to`: it is rebuilt under the new owner on
    /// the same footprint with its hit points (garrison ejected, queues lost).
    pub(crate) fn convert_building(&mut self, id: EntityId, to: PlayerId) {
        let Some((kind, footprint, span, hp)) = self
            .building(id)
            .map(|b| (b.kind, b.footprint, b.bridge_span, b.hp))
        else {
            return;
        };
        self.remove_building(id);
        if let Ok(new) = self.insert_building_shape(to, kind, footprint, true, span) {
            if let Some(b) = self.building_mut(new) {
                b.hp = hp.min(b.max_hp).max(1);
            }
            self.mark_building_changed(new);
        }
    }
}
