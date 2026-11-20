//! DAT salvo counts and spawning widths of land artillery and boarding ships.
use super::{Projectile, ProjectileKind, flight_ticks, projectile_roll};
use crate::{GameWorld, UnitKind};
use aoe_core::{FIXED_SUBUNITS_PER_TILE, Tick, WorldPosition};

impl GameWorld {
    pub(super) fn launch_volley(&mut self, shot: Projectile, speed: u16) {
        // IDs 280/550/588 use a separate projectile 369 with no attack
        // entries. IDs 2626/2627/2628 duplicate their primary hook.
        let (count, width) = match shot.source {
            Some(UnitKind::Mangonel) => (6, 100),
            Some(UnitKind::Onager) => (8, 125),
            Some(UnitKind::SiegeOnager) => (10, 150),
            Some(UnitKind::Hulk | UnitKind::WarHulk | UnitKind::Carrack) => (3, 100),
            _ => return,
        };
        let slots = self.economy.limits.projectiles - self.combat.projectiles.len();
        let secondary = shot.kind == ProjectileKind::Stone;
        let half_width = i64::from(FIXED_SUBUNITS_PER_TILE) * width / 200;
        let dx = i64::from(shot.to.x) - i64::from(shot.from.x);
        let dy = i64::from(shot.to.y) - i64::from(shot.from.y);
        let length = (dx * dx + dy * dy).isqrt().max(1);
        let clamp = |coordinate: i64, tiles| {
            coordinate.clamp(0, i64::from(tiles) * i64::from(FIXED_SUBUNITS_PER_TILE) - 1) as i32
        };
        for _ in 0..slots.min(count - 1) {
            let id = self.combat.next_projectile;
            let sample = projectile_roll(id, shot.attacker);
            let offset = |bits: u32| i64::from(bits) % (half_width * 2 + 1) - half_width;
            let across = offset(sample as u32);
            let forward = offset((sample >> 32) as u32);
            let to = WorldPosition::new(
                clamp(
                    i64::from(shot.to.x) + (dx * forward - dy * across) / length,
                    self.config.width_tiles,
                ),
                clamp(
                    i64::from(shot.to.y) + (dy * forward + dx * across) / length,
                    self.config.height_tiles,
                ),
            );
            let duration = flight_ticks(shot.from, to, speed, self.config.tick_hz);
            if !self.insert_projectile(Projectile {
                id,
                to,
                impact: Tick(shot.launched.0.saturating_add(duration)),
                secondary,
                attack: if secondary { 0 } else { shot.attack },
                bonus: if secondary { 0 } else { shot.bonus },
                ..shot
            }) {
                break;
            }
        }
    }
}
