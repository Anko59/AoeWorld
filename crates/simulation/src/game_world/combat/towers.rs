//! Defensive buildings share indexed targeting and the existing arrow system.
use super::{targeting::TargetArea, tick::Goal};
use crate::{BuildingKind, EconomyError, GameWorld};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId};
use std::ops::Bound::{Excluded, Unbounded};

impl GameWorld {
    /// Garrison firepower / single-arrow firepower, floored after summing.
    /// Villagers supply 5 damage / 2 s; foot archers use researched pierce DPS.
    /// Original rules: TC 0..10, tower 1..5, Castle 5..21 arrows.
    pub fn building_arrow_count(&self, id: EntityId) -> u8 {
        let Some(b) = self.building(id).filter(|b| b.is_complete() && b.hp > 0) else {
            return 0;
        };
        let Some(arrow) = self.building_attack(b.player, b.kind) else {
            return 0;
        };
        let (base, maximum) = match b.kind {
            BuildingKind::TownCenter => (0, 10),
            BuildingKind::WatchTower => (1, 5),
            BuildingKind::Castle => (5, 21),
            _ => return 0,
        };
        let researched = self.player_economy(b.player).map_or(0, |p| p.researched);
        // All current eligible reload periods divide 600 centiseconds exactly.
        let power: u32 = b
            .garrison
            .iter()
            .map(|g| match g.unit.kind {
                crate::UnitKind::Villager => {
                    3 * (5
                        + u32::from(self.has_researched(b.player, crate::Tech::Fletching))
                        + u32::from(self.has_researched(b.player, crate::Tech::BodkinArrow)))
                }
                crate::UnitKind::Archer
                | crate::UnitKind::Skirmisher
                | crate::UnitKind::Crossbowman
                | crate::UnitKind::EliteSkirmisher => {
                    let stats =
                        super::researched_stats(g.unit.kind, self.is_feudal(b.player), researched);
                    u32::from(stats.attack) * 600 / stats.attack_period_centiseconds
                }
                _ => 0,
            })
            .sum();
        (base + power / (u32::from(arrow.attack) * 600 / arrow.period_centiseconds)).min(maximum)
            as u8
    }

    pub(super) fn issue_building_attack(
        &mut self,
        player: PlayerId,
        id: EntityId,
        target: EntityId,
    ) -> Result<(), EconomyError> {
        let building = self.building(id).ok_or(EconomyError::UnknownEntity)?;
        if building.player != player {
            return Err(EconomyError::NotOwner);
        }
        if !building.is_complete() {
            return Err(EconomyError::UnderConstruction);
        }
        if self.building_arrow_count(id) == 0 {
            return Err(EconomyError::WrongUnitKind);
        }
        let arrow = self
            .building_attack(player, building.kind)
            .ok_or(EconomyError::WrongUnitKind)?;
        let area = self
            .building_target_area(id, arrow)
            .ok_or(EconomyError::UnknownEntity)?;
        let goal = self
            .attack_goal(target, player)
            .filter(|g| area.accepts(*g) && self.target_visible_to(player, target))
            .ok_or(EconomyError::InvalidTarget)?;
        let _ = goal;
        self.combat.tower_targets.insert(id, target);
        self.combat.tower_ready.entry(id).or_insert(0);
        Ok(())
    }

    fn building_target_area(
        &self,
        id: EntityId,
        arrow: super::BuildingAttack,
    ) -> Option<TargetArea> {
        let tower = self.building(id)?;
        let footprint = tower.footprint;
        Some(TargetArea {
            player: tower.player,
            // Ranges are measured from the building edge, as in AoE2.
            source: Goal::Building(footprint),
            min: i64::from(arrow.min_range) * i64::from(FIXED_SUBUNITS_PER_TILE),
            max: i64::from(arrow.range) * i64::from(FIXED_SUBUNITS_PER_TILE),
            actor: id,
            ram: false,
        })
    }

    pub(super) fn advance_towers(&mut self, _changed: &mut Vec<EntityId>) {
        // A fair bounded rotation, including empty/idle buildings. One dense
        // enemy crowd cannot make every tower scan the same world-wide list.
        let cursor = self.combat.tower_cursor;
        let towers: Vec<_> = self
            .combat
            .tower_ready
            .range((Excluded(cursor), Unbounded))
            .chain(self.combat.tower_ready.range(..=cursor))
            .map(|(&id, _)| id)
            .take(64)
            .collect();
        for id in towers {
            if self.combat.stats.targeting_scanned
                > super::targeting::MAX_TARGET_INSPECTIONS - super::targeting::MAX_QUERY_INSPECTIONS
            {
                break;
            }
            self.combat.tower_cursor = id;
            let Some(tower) = self.building(id).filter(|b| b.is_complete() && b.hp > 0) else {
                self.combat.tower_ready.remove(&id);
                self.combat.tower_targets.remove(&id);
                continue;
            };
            if self.combat.tower_ready[&id] > self.tick.0 {
                continue;
            }
            let arrows = self.building_arrow_count(id);
            if arrows == 0 {
                continue;
            }
            let Some(arrow) = self.building_attack(tower.player, tower.kind) else {
                continue;
            };
            let Some(area) = self.building_target_area(id, arrow) else {
                continue;
            };
            let manual = self
                .combat
                .tower_targets
                .get(&id)
                .copied()
                .filter(|&target| {
                    self.attack_goal(target, area.player)
                        .is_some_and(|g| area.accepts(g))
                        && self.target_visible_to(area.player, target)
                });
            if manual.is_none() {
                self.combat.tower_targets.remove(&id);
            }
            let target = manual.or_else(|| self.acquire_target(&area, true));
            if let Some(target) = target {
                self.combat.tower_ready.insert(
                    id,
                    self.tick.0
                        + (u64::from(arrow.period_centiseconds) * u64::from(self.config.tick_hz))
                            .div_ceil(100),
                );
                let bonus = self.unit(target).map_or(0, |u| {
                    if matches!(u.kind, crate::UnitKind::Spearman | crate::UnitKind::Pikeman) {
                        2
                    } else {
                        0
                    }
                });
                for _ in 0..arrows {
                    self.launch_projectile(
                        id,
                        target,
                        arrow.attack,
                        bonus,
                        super::ProjectileKind::Arrow,
                    );
                }
            }
        }
    }
}
