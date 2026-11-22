//! Delayed hits with fixed launch/aim points. No collision or accuracy dice.
use crate::GameWorld;
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, Tick, WorldPosition};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectileKind {
    Arrow,
    Javelin,
    Stone,
    Bolt,
    /// Fire streams and boarding hooks use the source's melee damage.
    Melee,
}

mod sources;
mod volleys;
pub use sources::{DatProjectile, dat_projectile};

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
    /// The launching unit's kind (`None` for buildings): blast, accuracy,
    /// pass-through and secondary-victim bonuses follow it.
    pub(crate) source: Option<crate::UnitKind>,
}
impl Projectile {
    pub fn source_kind(&self) -> Option<crate::UnitKind> {
        self.source
    }

    pub fn native_projectile_id(&self) -> Option<u16> {
        if self.is_secondary_stone() {
            Some(369)
        } else {
            self.source_kind().and_then(dat_projectile).map(|p| p.id)
        }
    }

    pub(super) fn is_secondary_stone(&self) -> bool {
        self.kind == ProjectileKind::Stone && self.secondary
    }

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
        let source = self.unit(attacker).map(|u| u.kind);
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
        // Licensed DE projectiles 363, 365 and 504 all move at 7 tiles/s.
        let speed = source.and_then(dat_projectile).map_or(
            if kind == ProjectileKind::Stone {
                350
            } else {
                700
            },
            |p| p.speed_hundredths,
        );
        let ticks = flight_ticks(from, to, speed, self.config.tick_hz);
        if !matches!(kind, ProjectileKind::Stone | ProjectileKind::Melee)
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
        let shot = Projectile {
            id: self.combat.next_projectile,
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
            source,
        };
        if self.insert_projectile(shot) && !secondary {
            self.launch_volley(shot, speed);
        }
    }

    fn insert_projectile(&mut self, shot: Projectile) -> bool {
        let Some(next) = shot.id.checked_add(1) else {
            return false;
        };
        self.combat.next_projectile = next;
        self.combat.projectiles.insert(shot.id, shot);
        self.combat.impacts.insert((shot.impact, shot.id));
        true
    }
    pub(super) fn advance_projectiles(&mut self, changed: &mut Vec<EntityId>) {
        debug_assert_eq!(self.combat.impacts.len(), self.combat.projectiles.len());
        let mut due = Vec::new();
        while let Some(&(impact, id)) = self.combat.impacts.first()
            && impact <= self.tick
        {
            self.combat.impacts.pop_first();
            due.push(id);
        }
        // In identity order, as a scan of `projectiles` takes them.
        due.sort_unstable();
        for id in due {
            if let Some(p) = self.combat.projectiles.remove(&id) {
                if p.kind == ProjectileKind::Stone {
                    self.resolve_splash(p, changed);
                } else if p.source.is_some_and(crate::UnitKind::is_scorpion) {
                    self.resolve_pass_through(p, changed);
                } else if self.projectile_misses(&p) {
                    // A miss lands harmlessly; DE's stray-hit rule is not modelled.
                } else if p.kind == ProjectileKind::Melee
                    || self.unit(p.target).is_none_or(|u| {
                        let dx = i64::from(u.position.x) - i64::from(p.to.x);
                        let dy = i64::from(u.position.y) - i64::from(p.to.y);
                        dx * dx + dy * dy <= i64::from(FIXED_SUBUNITS_PER_TILE / 2).pow(2)
                    })
                {
                    self.resolve_projectile_hit(p, changed);
                }
            }
        }
    }
}

fn flight_ticks(from: WorldPosition, to: WorldPosition, speed: u16, hz: u32) -> u64 {
    let dx = i64::from(to.x) - i64::from(from.x);
    let dy = i64::from(to.y) - i64::from(from.y);
    let square = (dx * dx + dy * dy) as u64;
    let floor = square.isqrt();
    let distance = floor + u64::from(floor * floor < square);
    (distance * u64::from(hz) * 100)
        .div_ceil(u64::from(speed) * FIXED_SUBUNITS_PER_TILE as u64)
        .max(1)
}

pub(super) fn projectile_roll(id: u64, attacker: EntityId) -> u64 {
    // SplitMix64 keyed by identity, independent of worker order.
    let mut z = (id ^ (u64::from(attacker.0) << 32)).wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}
