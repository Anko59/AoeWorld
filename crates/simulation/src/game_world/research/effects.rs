//! What a finished technology does to the world.
use super::{TECH_RULES, Tech, TechEffect};
use crate::{GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId};

/// Shared simulation/client presentation of the effects' integer carry limit.
pub fn villager_capacity(researched: u128) -> u16 {
    carry_capacity(researched, crate::VILLAGE_RULES.villager_capacity)
}

pub(crate) fn carry_capacity(researched: u128, base: u16) -> u16 {
    (u32::from(base) * villager_economy_modifiers(researched).0).div_ceil(10_000) as u16
}

/// Walking speed percent of villagers with these technologies.
pub(crate) fn villager_speed_percent(researched: u128) -> u16 {
    villager_economy_modifiers(researched).1
}

/// Asked once per walking villager per tick, so only the few technologies with
/// this effect are visited, in `TECH_RULES` order.
fn villager_economy_modifiers(researched: u128) -> (u32, u16) {
    static RULES: std::sync::OnceLock<Vec<(u128, u32, u16)>> = std::sync::OnceLock::new();
    RULES
        .get_or_init(|| {
            TECH_RULES
                .iter()
                .filter_map(|r| match r.effect {
                    TechEffect::VillagerEconomy {
                        capacity_permyriad,
                        speed_percent,
                    } => Some((r.tech.bit(), capacity_permyriad, speed_percent)),
                    _ => None,
                })
                .collect()
        })
        .iter()
        .filter(|(bit, _, _)| researched & bit != 0)
        .fold((10_000, 100), |total, (_, capacity, speed)| {
            (total.0 * capacity / 10_000, total.1 * speed / 100)
        })
}

impl GameWorld {
    pub fn is_feudal(&self, player: PlayerId) -> bool {
        self.player_economy(player)
            .is_some_and(|p| p.age >= super::Age::Feudal)
    }

    /// Integer resources use the ceiling of DE's fractional carry limit.
    pub fn villager_capacity(&self, player: PlayerId) -> u16 {
        villager_capacity(self.player_economy(player).map_or(0, |p| p.researched))
    }

    /// Hundredths of a resource/second as an exact rational, avoiding rounding
    /// 0.39 * 1.20, 0.38 * 1.15 and 0.36 * 1.15 to whole hundredths.
    pub fn gather_rate(&self, player: PlayerId, resource: aoe_map::ResourceKind) -> (u32, u32) {
        let researched = self.player_economy(player).map_or(0, |p| p.researched);
        let percent = TECH_RULES
            .iter()
            .filter(|r| researched & r.tech.bit() != 0)
            .fold(10_000_u32, |total, r| match r.effect {
                TechEffect::GatherRate {
                    resource: kind,
                    percent,
                } if resource == kind => total * u32::from(percent) / 100,
                _ => total,
            });
        let base = u32::from(crate::VILLAGE_RULES.gather_hundredths_per_second[resource as usize]);
        if percent.is_multiple_of(100) {
            (base * (percent / 100), 100)
        } else {
            (base * percent, 10_000)
        }
    }

    pub(crate) fn farm_food(&self, player: PlayerId) -> u16 {
        TECH_RULES
            .iter()
            .filter(|r| self.has_researched(player, r.tech))
            .fold(crate::BuildingKind::Farm.rules().food, |total, r| {
                match r.effect {
                    TechEffect::FarmFood(bonus) => total.saturating_add(bonus),
                    _ => total,
                }
            })
    }

    /// Summed villager `(hp, melee armour, pierce armour)` bonuses of every
    /// technology `player` has researched; zero for other unit kinds.
    pub(crate) fn villager_bonus(&self, player: PlayerId, kind: UnitKind) -> (u16, i16, u16) {
        let mut total = (0, 0, 0);
        if kind != UnitKind::Villager {
            return total;
        }
        for rules in &TECH_RULES {
            if let TechEffect::VillagerBonus {
                hp,
                melee_armour,
                pierce_armour,
            } = rules.effect
                && self.has_researched(player, rules.tech)
            {
                total = (
                    total.0 + hp,
                    total.1 + melee_armour,
                    total.2 + pierce_armour,
                );
            }
        }
        total
    }

    /// Line-of-sight tiles added to every building of `player`.
    pub(crate) fn researched_los_bonus(&self, player: PlayerId) -> u8 {
        TECH_RULES
            .iter()
            .filter(|rules| self.has_researched(player, rules.tech))
            .map(|rules| match rules.effect {
                TechEffect::BuildingLos(bonus) => bonus,
                _ => 0,
            })
            .fold(0, u8::saturating_add)
    }

    pub(in crate::game_world) fn complete_research(
        &mut self,
        player: PlayerId,
        tech: Tech,
        changed: &mut Vec<EntityId>,
    ) {
        // Rates and carry capacity change: dozing workers re-check.
        self.combat.quiet.clear();
        self.combat.healing.wake_all();
        self.wake_player_gatherers(player);
        let sites = self
            .player_buildings(player)
            .map(|b| b.id)
            .collect::<Vec<_>>();
        for id in sites {
            self.wake_construction(id);
            self.wake_queues(id);
        }
        let Some(state) = self.economy.players.get_mut(&player) else {
            return;
        };
        state.researched |= tech.bit();
        match tech.rules().effect {
            TechEffect::UpgradeGalley => self.upgrade_galleys(player, changed),
            TechEffect::Military => {
                if matches!(tech, Tech::Fletching | Tech::BodkinArrow | Tech::Bracer) {
                    for b in self.economy.buildings.values_mut().filter(|b| {
                        b.player == player
                            && matches!(
                                b.kind,
                                crate::BuildingKind::TownCenter
                                    | crate::BuildingKind::WatchTower
                                    | crate::BuildingKind::Castle
                            )
                    }) {
                        b.los += 1;
                        self.vision.dirty_buildings.insert(b.id);
                    }
                }
            }
            TechEffect::Masonry | TechEffect::BuildingStats => {
                self.refresh_building_hp(player);
            }
            TechEffect::UnitSpeed { .. } | TechEffect::Passive => {}
            TechEffect::UnitHp { target, hp } => {
                for stored in &mut self.units {
                    let unit = &mut stored.state;
                    if unit.player == player && target.includes(unit.kind) && unit.hp > 0 {
                        unit.max_hp = unit.max_hp.saturating_add(hp);
                        unit.hp = unit.hp.saturating_add(hp);
                        changed.push(unit.id);
                    }
                }
            }
            TechEffect::UpgradeUnit { from, to } => {
                self.upgrade_units(player, from, to, changed);
            }
            TechEffect::UpgradeMilitia => {
                self.upgrade_units(player, UnitKind::Militia, UnitKind::ManAtArms, changed);
            }
            TechEffect::GatherRate { .. }
            | TechEffect::FarmFood(_)
            | TechEffect::VillagerEconomy { .. } => {}
            TechEffect::AgeUp(age) => {
                state.age = age;
                for building in self
                    .economy
                    .buildings
                    .values_mut()
                    .filter(|b| b.player == player)
                {
                    building.age = age;
                    self.vision.dirty_buildings.insert(building.id);
                    let los = building.kind.age_stats(age).1;
                    let hp = super::building_max_hp(building.kind, age, state.researched);
                    let (_, old_los) = building.kind.age_stats(match age {
                        super::Age::Imperial => super::Age::Castle,
                        super::Age::Castle => super::Age::Feudal,
                        _ => super::Age::Dark,
                    });
                    let bonus = hp.saturating_sub(building.max_hp);
                    let earned = if building.is_complete() {
                        bonus
                    } else {
                        (u64::from(bonus) * building.work_done / building.work_total.max(1)) as u16
                    };
                    building.hp = building.hp.saturating_add(earned).min(hp);
                    building.max_hp = hp;
                    building.los = building.los.saturating_add(los.saturating_sub(old_los));
                }
            }
            TechEffect::BuildingLos(bonus) => {
                for building in self.economy.buildings.values_mut() {
                    if building.player == player {
                        building.los = building.los.saturating_add(bonus);
                        self.vision.dirty_buildings.insert(building.id);
                    }
                }
            }
            TechEffect::VillagerBonus { hp, .. } => {
                // Damage already taken is kept: both values rise together.
                for stored in &mut self.units {
                    let unit = &mut stored.state;
                    if unit.player == player && unit.kind == UnitKind::Villager && unit.hp > 0 {
                        unit.max_hp = unit.max_hp.saturating_add(hp);
                        unit.hp = unit.hp.saturating_add(hp);
                        changed.push(unit.id);
                    }
                }
            }
        }
        match tech {
            Tech::Conscription => self.speed_up_training(player),
            Tech::TreadmillCrane => self.speed_up_construction(player),
            _ => {}
        }
        self.upgrade_garrisoned_units(player, tech);
        if matches!(
            tech,
            Tech::TownWatch
                | Tech::TownPatrol
                | Tech::FeudalAge
                | Tech::CastleAge
                | Tech::ImperialAge
                | Tech::Fletching
                | Tech::BodkinArrow
                | Tech::Bracer
        ) {
            let ids = self
                .buildings()
                .filter(|b| b.player == player)
                .map(|b| b.id)
                .collect::<Vec<_>>();
            for id in ids {
                self.update_building_vision(id);
            }
        }
        if matches!(
            tech,
            Tech::FeudalAge
                | Tech::CastleAge
                | Tech::ImperialAge
                | Tech::Fletching
                | Tech::BodkinArrow
                | Tech::Bracer
        ) {
            let ids = self
                .units()
                .filter(|u| {
                    u.player == player
                        && matches!(
                            u.kind,
                            UnitKind::Scout
                                | UnitKind::Archer
                                | UnitKind::Skirmisher
                                | UnitKind::Crossbowman
                                | UnitKind::EliteSkirmisher
                                | UnitKind::Arbalester
                                | UnitKind::Galley
                                | UnitKind::WarGalley
                        )
                })
                .map(|u| u.id)
                .collect::<Vec<_>>();
            for id in ids {
                self.update_unit_vision(id);
            }
        }
    }
}
