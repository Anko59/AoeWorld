//! What a finished technology does to the world.
use super::{TECH_RULES, Tech, TechEffect};
use crate::{GameUnit, GameWorld, UnitKind};
use aoe_core::PlayerId;

impl GameWorld {
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

    pub(super) fn complete_research(
        &mut self,
        player: PlayerId,
        tech: Tech,
        changed: &mut Vec<GameUnit>,
    ) {
        let Some(state) = self.economy.players.get_mut(&player) else {
            return;
        };
        state.researched |= tech.bit();
        match tech.rules().effect {
            TechEffect::AgeUp(age) => state.age = age,
            TechEffect::BuildingLos(bonus) => {
                for building in self.economy.buildings.values_mut() {
                    if building.player == player {
                        building.los = building.los.saturating_add(bonus);
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
                        changed.push(*unit);
                    }
                }
            }
        }
        if tech == Tech::TownWatch {
            let ids = self
                .buildings()
                .filter(|b| b.player == player)
                .map(|b| b.id)
                .collect::<Vec<_>>();
            for id in ids {
                self.update_building_vision(id);
            }
        }
        if tech == Tech::FeudalAge {
            let ids = self
                .units()
                .filter(|u| u.player == player && u.kind == UnitKind::Scout)
                .map(|u| u.id)
                .collect::<Vec<_>>();
            for id in ids {
                self.update_unit_vision(id);
            }
        }
    }
}
