//! Deterministic demolition blast over the bounded spatial query.
use crate::{DamageKind, GameWorld, UnitKind, unit_combat};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, TileRect};

impl GameWorld {
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
            let bonus = self.unit_damage_bonus(UnitKind::DemolitionShip, id);
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
