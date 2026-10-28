//! Deterministic sub-tile positions around a target's static footprint.
use crate::{GameWorld, UnitKind};
use aoe_core::{EntityId, TileRect, WorldPosition};

pub(crate) fn squared(a: WorldPosition, b: WorldPosition) -> i64 {
    let x = i64::from(a.x) - i64::from(b.x);
    let y = i64::from(a.y) - i64::from(b.y);
    x * x + y * y
}

/// Equal steps around an expanded rectangle. The small corner stand-off is
/// inside the villager's 0.1-tile work reach; ship work reach covers its bow.
/// Even the largest building has fewer than 64 candidates.
pub(crate) fn perimeter_positions(rect: TileRect, kind: UnitKind) -> Vec<WorldPosition> {
    let r = kind.collision_radius() + 8;
    let min = WorldPosition::new(rect.min.x * 1024 - r, rect.min.y * 1024 - r);
    let max = WorldPosition::new(rect.max.x * 1024 + r, rect.max.y * 1024 + r);
    let spacing = kind.collision_radius() * 2 + 16;
    let mut points = Vec::new();
    for (from, to) in [
        (min, WorldPosition::new(max.x, min.y)),
        (WorldPosition::new(max.x, min.y), max),
        (max, WorldPosition::new(min.x, max.y)),
        (WorldPosition::new(min.x, max.y), min),
    ] {
        let length = (to.x - from.x).abs().max((to.y - from.y).abs());
        let count = (length / spacing).clamp(1, 16);
        for n in 0..count {
            points.push(WorldPosition::new(
                from.x + (to.x - from.x) * n / count,
                from.y + (to.y - from.y) * n / count,
            ));
        }
    }
    points
}

pub(crate) fn footprint_distance_squared(at: WorldPosition, rect: TileRect) -> i64 {
    let x = at.x.clamp(rect.min.x * 1024, rect.max.x * 1024);
    let y = at.y.clamp(rect.min.y * 1024, rect.max.y * 1024);
    squared(at, WorldPosition::new(x, y))
}

impl GameWorld {
    pub(crate) fn interaction_point_free(&self, id: EntityId, at: WorldPosition) -> bool {
        let Some(&index) = self.lookup.get(&id) else {
            return false;
        };
        let kind = self.units[index].state.kind;
        self.config.valid_ground_position(at)
            && !self.economy.blocked_tiles.contains(&at.tile_floor())
            && self
                .unit_tile_passable(kind, at.tile_floor(), &|| false)
                .unwrap_or(false)
            && self.body_position_clear(kind, at)
            && !self.destination_point_reserved(at, id)
            && !self.position_occupied(index, at, true)
    }

    pub(crate) fn at_work_position(&self, id: EntityId, tile: aoe_core::TileCoord) -> bool {
        let Some(unit) = self.unit(id) else {
            return false;
        };
        let range =
            i64::from(unit.kind.collision_radius() + if unit.kind.is_naval() { 256 } else { 104 });
        let rect = TileRect::from_xywh(tile.x, tile.y, 1, 1);
        !rect.contains(unit.position.tile_floor())
            && footprint_distance_squared(unit.position, rect) <= range * range
    }
}
