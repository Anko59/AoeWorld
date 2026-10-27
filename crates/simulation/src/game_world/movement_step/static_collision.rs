//! Exact short swept circles against static tile footprints. Endpoint-only
//! sampling accepts chords through a corner that actual small steps reject.
use crate::{GameWorld, UnitKind};
use aoe_core::{TileCoord, WorldPosition};

fn point_distance(p: WorldPosition, tile: TileCoord) -> i64 {
    let x = i64::from(p.x);
    let y = i64::from(p.y);
    let a = i64::from(tile.x) * 1024;
    let b = i64::from(tile.y) * 1024;
    (x - x.clamp(a, a + 1024)).pow(2) + (y - y.clamp(b, b + 1024)).pow(2)
}

fn crosses_rectangle(a: WorldPosition, b: WorldPosition, tile: TileCoord) -> bool {
    let (mut low, mut high) = ((0_i64, 1_i64), (1_i64, 1_i64));
    for (p, q, edge) in [(a.x, b.x, tile.x), (a.y, b.y, tile.y)] {
        let p = i64::from(p);
        let d = i64::from(q) - p;
        let edge = i64::from(edge) * 1024;
        if d == 0 {
            if p < edge || p > edge + 1024 {
                return false;
            }
            continue;
        }
        let mut first = (edge - p) * d.signum();
        let mut last = (edge + 1024 - p) * d.signum();
        if first > last {
            std::mem::swap(&mut first, &mut last);
        }
        let den = d.abs();
        if first * low.1 > low.0 * den {
            low = (first, den);
        }
        if last * high.1 < high.0 * den {
            high = (last, den);
        }
        if low.0 * high.1 > high.0 * low.1 {
            return false;
        }
    }
    true
}

fn sweeps_corner(a: WorldPosition, b: WorldPosition, tile: TileCoord, radius: i64) -> bool {
    if crosses_rectangle(a, b, tile) {
        return true;
    }
    let dx = i64::from(b.x) - i64::from(a.x);
    let dy = i64::from(b.y) - i64::from(a.y);
    let length = dx * dx + dy * dy;
    for y in [i64::from(tile.y) * 1024, (i64::from(tile.y) + 1) * 1024] {
        for x in [i64::from(tile.x) * 1024, (i64::from(tile.x) + 1) * 1024] {
            let x = x - i64::from(a.x);
            let y = y - i64::from(a.y);
            let dot = x * dx + y * dy;
            let cross = i128::from(x) * i128::from(dy) - i128::from(y) * i128::from(dx);
            if dot > 0
                && dot < length
                && cross * cross < i128::from(radius * radius) * i128::from(length)
            {
                return true;
            }
        }
    }
    false
}

impl GameWorld {
    pub(super) fn body_step_clear(
        &self,
        kind: UnitKind,
        from: WorldPosition,
        to: WorldPosition,
    ) -> bool {
        if !self.body_position_clear(kind, to) {
            return false;
        }
        // A half-tile axis step cannot pass a whole solid tile between two
        // clear endpoints. Diagonal edges additionally need the corner sweep.
        if from.x == to.x || from.y == to.y {
            return true;
        }
        let r = kind.collision_radius();
        let min = WorldPosition::new(from.x.min(to.x) - r, from.y.min(to.y) - r).tile_floor();
        let max = WorldPosition::new(from.x.max(to.x) + r, from.y.max(to.y) + r).tile_floor();
        let mut bridge = None;
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let tile = TileCoord::new(x, y);
                if tile == from.tile_floor()
                    || tile == to.tile_floor()
                    || point_distance(from, tile) < i64::from(r).pow(2)
                    || point_distance(to, tile) < i64::from(r).pow(2)
                    || !sweeps_corner(from, to, tile, i64::from(r))
                {
                    continue;
                }
                if self.economy.blocked_tiles.contains(&tile) {
                    return false;
                }
                if !self
                    .unit_tile_passable(kind, tile, &|| false)
                    .unwrap_or(false)
                    && !(*bridge.get_or_insert_with(|| {
                        !kind.is_naval()
                            && self.on_bridge_corridor(from, r)
                            && self.on_bridge_corridor(to, r)
                    }) && self.bridge_water_clear(tile))
                {
                    return false;
                }
            }
        }
        true
    }
}
