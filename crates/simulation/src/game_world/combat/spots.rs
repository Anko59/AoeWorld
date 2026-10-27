//! Melee endpoints are free circles on the target perimeter, reached via the
//! ordinary static attack search. Ranged searches retain their range boundary.
use super::tick::Goal;
use crate::game_world::movement_step::slots::{perimeter_positions, squared};
use crate::{GameWorld, UnitKind};
use aoe_core::{EntityId, TileCoord, WorldPosition};

impl GameWorld {
    pub(super) fn melee_positions(&self, id: EntityId, goal: &Goal) -> Vec<WorldPosition> {
        let Some(unit) = self.unit(id) else {
            return Vec::new();
        };
        let Some(stats) = self.unit_stats(id) else {
            return Vec::new();
        };
        let range = super::super::navy::attack_range(&stats);
        let mut points = match goal {
            Goal::Building(rect) => perimeter_positions(*rect, unit.kind),
            Goal::Unit(at, target_radius) => circle_positions(
                *at,
                (unit.kind.collision_radius() + target_radius + 8).min(range as i32),
            ),
        };
        let valid = |at: &WorldPosition| {
            let at = *at;
            goal.distance_squared(at) <= range * range
                && self.config.valid_ground_position(at)
                && !self.economy.blocked_tiles.contains(&at.tile_floor())
                && self
                    .unit_tile_passable(unit.kind, at.tile_floor(), &|| false)
                    .unwrap_or(false)
                && self.body_position_clear(unit.kind, at)
        };
        points.retain(valid);
        // A target can stand inside newly blocked ground. Its contact ring
        // is then obstructed too; use the nearest legal firing reach instead.
        if points.is_empty()
            && let Goal::Unit(at, _) = goal
        {
            points = circle_positions(*at, range as i32);
            points.retain(valid);
        }
        points.sort_unstable_by_key(|&at| (squared(unit.position, at), at.y, at.x));
        points
    }

    /// A bounded static ray proves a cheap nearby endpoint. Other endpoints
    /// use the shared full-search budget instead of unbounded rays.
    pub(super) fn attack_line_clear(
        &self,
        kind: UnitKind,
        from: WorldPosition,
        to: WorldPosition,
    ) -> bool {
        let a = from.tile_floor();
        let b = to.tile_floor();
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let count = dx.abs().max(dy.abs());
        if count > 64 {
            return false;
        }
        let mut previous = a;
        for n in 1..=count.max(1) {
            let tile = TileCoord::new(a.x + dx * n / count.max(1), a.y + dy * n / count.max(1));
            if !self.building_step_clear(previous, tile)
                || !self.unit_step_clear(kind, previous, tile).unwrap_or(false)
            {
                return false;
            }
            previous = tile;
        }
        true
    }

    pub(super) fn uses_melee_position(kind: UnitKind) -> bool {
        let stats = super::unit_combat(kind);
        stats.attack_kind == super::DamageKind::Melee && stats.range <= 1
    }
}

fn circle_positions(center: WorldPosition, radius: i32) -> Vec<WorldPosition> {
    const DIRECTIONS: [(i32, i32); 12] = [
        (1024, 0),
        (886, 512),
        (512, 886),
        (0, 1024),
        (-512, 886),
        (-886, 512),
        (-1024, 0),
        (-886, -512),
        (-512, -886),
        (0, -1024),
        (512, -886),
        (886, -512),
    ];
    DIRECTIONS
        .into_iter()
        .map(|(x, y)| {
            WorldPosition::new(center.x + x * radius / 1024, center.y + y * radius / 1024)
        })
        .collect()
}
