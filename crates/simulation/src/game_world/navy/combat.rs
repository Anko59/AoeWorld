//! Deterministic demolition blast over the bounded spatial query.
use crate::{DamageKind, GameWorld, UnitKind, unit_combat};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, TileRect};

impl GameWorld {
    pub(crate) fn naval_damage_bonus(&self, attacker: UnitKind, target: EntityId) -> u16 {
        let Some(attack) = super::stats::damage_stats(attacker) else {
            return 0;
        };
        let (mut extra, melee_armour) = if let Some(unit) = self.unit(target) {
            let ship_armour = super::stats::damage_stats(unit.kind).map_or(0, |s| s.ship_armour);
            let bonus = if unit.kind.is_naval() {
                attack.ship_attack.saturating_sub(ship_armour)
            } else {
                0
            };
            (bonus, self.unit_armour(target).map_or(0, |a| a.0))
        } else if let Some(building) = self.building(target) {
            (
                attack.building_attack,
                super::super::combat::age_armour(building.kind, building.age).melee_armour,
            )
        } else {
            (0, 0)
        };
        if attack.melee_attack > 0 {
            extra = extra.saturating_add(
                (i32::from(attack.melee_attack) - i32::from(melee_armour)).max(0) as u16,
            );
        }
        extra
    }

    pub(crate) fn detonate(
        &mut self,
        attacker: EntityId,
        target: EntityId,
        changed: &mut Vec<EntityId>,
    ) {
        let Some(ship) = self.unit(attacker) else {
            return;
        };
        if self.attack_goal(target, ship.player).is_none() {
            return;
        }
        let center = ship.position;
        let tile = center.tile_floor();
        let Some(stats) = super::stats::damage_stats(UnitKind::DemolitionShip) else {
            return;
        };
        let hundredths = stats.blast_hundredths;
        let radius = i64::from(FIXED_SUBUNITS_PER_TILE) * i64::from(hundredths) / 100;
        let rect = TileRect::from_xywh(
            (tile.x - 3).max(0),
            (tile.y - 3).max(0),
            (tile.x + 4).min(self.config.width_tiles) - (tile.x - 3).max(0),
            (tile.y + 4).min(self.config.height_tiles) - (tile.y - 3).max(0),
        );
        let mut targets = self
            .query(rect)
            .0
            .into_iter()
            .filter(|u| u.hp > 0 && u.player != ship.player)
            .filter(|u| {
                let dx = i64::from(u.position.x) - i64::from(center.x);
                let dy = i64::from(u.position.y) - i64::from(center.y);
                dx * dx + dy * dy <= radius * radius
            })
            .map(|u| u.id)
            .collect::<Vec<_>>();
        targets.extend(
            self.buildings()
                .filter(|b| b.player != ship.player && b.hp > 0)
                .filter(|b| {
                    super::super::combat::tick::Goal::Building(b.footprint).distance_squared(center)
                        <= radius * radius
                })
                .map(|b| b.id),
        );
        targets.sort_unstable();
        for id in targets {
            let bonus = self.naval_damage_bonus(UnitKind::DemolitionShip, id);
            self.resolve_hit(
                (attacker, ship.player),
                id,
                unit_combat(UnitKind::DemolitionShip).attack,
                bonus,
                DamageKind::Melee,
                changed,
            );
        }
        if let Some(&index) = self.lookup.get(&attacker) {
            self.kill_unit(index);
            changed.push(attacker);
        }
    }
}
