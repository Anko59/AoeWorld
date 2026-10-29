//! Delayed hits with fixed launch/aim points. No collision or accuracy dice.
use crate::GameWorld;
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, Tick, WorldPosition};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectileKind {
    Arrow,
    Javelin,
    Stone,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Projectile {
    pub id: u64,
    pub player: PlayerId,
    pub attacker: EntityId,
    pub target: EntityId,
    pub from: WorldPosition,
    pub to: WorldPosition,
    pub launched: Tick,
    pub impact: Tick,
    pub kind: ProjectileKind,
    pub(crate) attack: u16,
    pub(crate) bonus: u16,
    pub(crate) secondary: bool,
}
impl Projectile {
    pub fn position(&self, tick: Tick) -> WorldPosition {
        let duration = self.impact.0.saturating_sub(self.launched.0).max(1);
        let elapsed = tick.0.saturating_sub(self.launched.0).min(duration);
        let axis = |a: i32, b: i32| {
            (i64::from(a) + (i64::from(b) - i64::from(a)) * elapsed as i64 / duration as i64) as i32
        };
        WorldPosition::new(axis(self.from.x, self.to.x), axis(self.from.y, self.to.y))
    }
}

impl GameWorld {
    pub fn projectiles(&self) -> impl Iterator<Item = &Projectile> {
        self.combat.projectiles.values()
    }
    pub fn visible_projectiles(&self, player: PlayerId) -> impl Iterator<Item = &Projectile> {
        self.projectiles().filter(move |p| {
            self.visibility(player, p.position(self.tick).tile_floor())
                == crate::TileVisibility::Visible
        })
    }
    pub(super) fn launch_projectile(
        &mut self,
        attacker: EntityId,
        target: EntityId,
        damage: (u16, u16),
        kind: ProjectileKind,
        secondary: bool,
    ) {
        if self.combat.projectiles.len() >= self.economy.limits.projectiles {
            return;
        }
        let Some((player, from)) =
            self.unit(attacker)
                .map(|u| (u.player, u.position))
                .or_else(|| {
                    self.building(attacker).map(|b| {
                        (
                            b.player,
                            WorldPosition::new(
                                (b.footprint.min.x + b.footprint.max.x) * FIXED_SUBUNITS_PER_TILE
                                    / 2,
                                (b.footprint.min.y + b.footprint.max.y) * FIXED_SUBUNITS_PER_TILE
                                    / 2,
                            ),
                        )
                    })
                })
        else {
            return;
        };
        if !self.target_visible_to(player, target) {
            return;
        }
        let Some(mut to) = self.attack_goal(target, player).map(|g| g.centre()) else {
            return;
        };
        let dx = i64::from(to.x) - i64::from(from.x);
        let dy = i64::from(to.y) - i64::from(from.y);
        let distance = (dx * dx + dy * dy) as u64;
        let distance = distance.isqrt() + u64::from(distance.isqrt().pow(2) < distance);
        // Licensed DE projectiles 363, 365 and 504 all move at 7 tiles/s.
        let speed = if kind == ProjectileKind::Stone {
            35
        } else {
            70
        };
        let ticks = (distance * u64::from(self.config.tick_hz) * 10)
            .div_ceil(speed * FIXED_SUBUNITS_PER_TILE as u64)
            .max(1);
        if kind != ProjectileKind::Stone
            && self.has_researched(player, crate::Tech::Ballistics)
            && let Some(u) = self.unit(target).filter(|u| u.moving)
        {
            let lead = |now: i32, previous: i32, limit: i32| {
                (i64::from(now) + (i64::from(now) - i64::from(previous)) * ticks as i64)
                    .clamp(0, i64::from(limit) * i64::from(FIXED_SUBUNITS_PER_TILE) - 1)
                    as i32
            };
            to = WorldPosition::new(
                lead(u.position.x, u.previous_position.x, self.config.width_tiles),
                lead(
                    u.position.y,
                    u.previous_position.y,
                    self.config.height_tiles,
                ),
            );
        }
        let id = self.combat.next_projectile;
        let Some(next) = id.checked_add(1) else {
            return;
        };
        self.combat.next_projectile = next;
        self.combat.projectiles.insert(
            id,
            Projectile {
                id,
                player,
                attacker,
                target,
                from,
                to,
                launched: self.tick,
                impact: Tick(self.tick.0.saturating_add(ticks)),
                kind,
                attack: damage.0,
                bonus: damage.1,
                secondary,
            },
        );
    }
    pub(super) fn advance_projectiles(&mut self, changed: &mut Vec<EntityId>) {
        let due: Vec<_> = self
            .combat
            .projectiles
            .values()
            .filter(|p| p.impact <= self.tick)
            .map(|p| p.id)
            .collect();
        for id in due {
            if let Some(p) = self.combat.projectiles.remove(&id) {
                if p.kind == ProjectileKind::Stone {
                    self.resolve_splash(p, changed);
                } else if self.unit(p.target).is_none_or(|u| {
                    let dx = i64::from(u.position.x) - i64::from(p.to.x);
                    let dy = i64::from(u.position.y) - i64::from(p.to.y);
                    dx * dx + dy * dy <= i64::from(FIXED_SUBUNITS_PER_TILE / 2).pow(2)
                }) {
                    self.resolve_projectile_hit(p, changed);
                }
            }
        }
    }
}
