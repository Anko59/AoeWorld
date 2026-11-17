//! Deterministic demolition blast over the bounded spatial query.
use crate::{DamageKind, GameWorld, unit_combat};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, TileRect};

const INCENDIARY_RADIUS_HUNDREDTHS: i64 = 200;
const INCENDIARY_DAMAGE: u16 = 30;

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
        let Some(stats) = super::stats::damage_stats(ship.kind) else {
            return;
        };
        let hundredths = stats.blast_hundredths;
        let radius = i64::from(FIXED_SUBUNITS_PER_TILE) * i64::from(hundredths) / 100;
        let rect = TileRect::from_xywh(
            (tile.x - 4).max(0),
            (tile.y - 4).max(0),
            (tile.x + 5).min(self.config.width_tiles) - (tile.x - 4).max(0),
            (tile.y + 5).min(self.config.height_tiles) - (tile.y - 4).max(0),
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
            let bonus = self.unit_damage_bonus(ship.kind, id);
            self.resolve_hit(
                (attacker, ship.player),
                id,
                unit_combat(ship.kind).attack,
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

    /// Incendiaries: a sunk Fire Ship detonates, hurting enemy units within
    /// two tiles. DE gives no figures (patch notes: "detonate when sunk, gaining
    /// blast radius"); radius and damage are this project's stated assumption,
    /// see docs/navy.md. The blast may sink further Fire Ships, which blast in
    /// turn on the next pass; every ship blasts at most once.
    pub(crate) fn run_incendiary_blasts(&mut self, changed: &mut Vec<EntityId>) {
        while !self.combat.blasts.is_empty() {
            let blasts = std::mem::take(&mut self.combat.blasts);
            for (ship, player, center) in blasts {
                let radius =
                    i64::from(FIXED_SUBUNITS_PER_TILE) * INCENDIARY_RADIUS_HUNDREDTHS / 100;
                let tile = center.tile_floor();
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
                    .filter(|u| u.hp > 0 && u.player != player)
                    .filter(|u| {
                        let dx = i64::from(u.position.x) - i64::from(center.x);
                        let dy = i64::from(u.position.y) - i64::from(center.y);
                        dx * dx + dy * dy <= radius * radius
                    })
                    .map(|u| u.id)
                    .collect::<Vec<_>>();
                targets.sort_unstable();
                for id in targets {
                    self.resolve_hit(
                        (ship, player),
                        id,
                        INCENDIARY_DAMAGE,
                        0,
                        DamageKind::Melee,
                        changed,
                    );
                }
            }
        }
    }
}
