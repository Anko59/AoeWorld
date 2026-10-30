//! Static clearance along continuous raster bridge decks.
use crate::{GameWorld, Terrain, UnitKind};
use aoe_core::{TileCoord, WorldPosition};

impl GameWorld {
    /// Physical steps on a diagonal deck may cross a water cell before crossing
    /// the next raster cell's other axis. Tile connectivity guides the global
    /// route; it must not reject a swept body supported by the continuous deck.
    pub(crate) fn unit_position_step_clear(
        &self,
        kind: UnitKind,
        from: WorldPosition,
        to: WorldPosition,
    ) -> Result<bool, aoe_map::EnvironmentPageError> {
        if self.unit_step_clear(kind, from.tile_floor(), to.tile_floor())? {
            return Ok(true);
        }
        let radius = kind.terrain_radius();
        Ok(!kind.is_naval()
            && self.on_bridge_corridor(from, radius)
            && self.on_bridge_corridor(to, radius)
            && [from, to].into_iter().all(|point| {
                let tile = point.tile_floor();
                self.unit_tile_passable(kind, tile, &|| false)
                    .unwrap_or(false)
                    || self.bridge_water_clear(tile)
            })
            && self.body_step_clear(kind, from, to))
    }

    // Raster bridge links form a one-tile-wide continuous deck. On a diagonal
    // link its sides cross water tile corners: water there is not a solid wall.
    // Foundations/buildings are still checked independently above.
    pub(in crate::game_world::movement_step) fn bridge_water_clear(&self, tile: TileCoord) -> bool {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return false;
        };
        !overlay.blocks_tile(tile)
            && generator
                .tile_and_node_with_cancel(tile, &|| false)
                .ok()
                .flatten()
                .is_some_and(|(t, node)| {
                    t.water != aoe_map::WaterKind::None
                        && node.is_none_or(|n| !overlay.blocks_node(n))
                })
    }

    pub(in crate::game_world::movement_step) fn on_bridge_corridor(
        &self,
        at: WorldPosition,
        radius: i32,
    ) -> bool {
        let Terrain::Map { overlay, .. } = &self.terrain else {
            return false;
        };
        let here = at.tile_floor();
        let margin = i128::from(512 - radius);
        for y in -1..=1 {
            for x in -1..=1 {
                let from = TileCoord::new(here.x + x, here.y + y);
                let Some(mask) = overlay.bridge_connections(from) else {
                    continue;
                };
                let Ok(start) = WorldPosition::from_tile_center(from) else {
                    continue;
                };
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if mask & (1 << ((dy + 1) * 3 + dx + 1)) == 0 {
                            continue;
                        }
                        let to = TileCoord::new(from.x + dx, from.y + dy);
                        if overlay.bridge_edge(from, to) != Some(true) {
                            continue;
                        }
                        let vx = i128::from(dx * 1024);
                        let vy = i128::from(dy * 1024);
                        let px = i128::from(at.x - start.x);
                        let py = i128::from(at.y - start.y);
                        let len = vx * vx + vy * vy;
                        let dot = px * vx + py * vy;
                        if len > 0
                            && dot >= 0
                            && dot <= len
                            && (px * vy - py * vx).pow(2) <= margin * margin * len
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

type Fraction = (i128, i128);
fn less(a: Fraction, b: Fraction) -> bool {
    a.0 * b.1 < b.0 * a.1
}
fn clip(
    origin: i128,
    delta: i128,
    bounds: (i128, i128),
    interval: &mut (Fraction, Fraction),
) -> bool {
    if delta == 0 {
        return bounds.0 <= origin && origin <= bounds.1;
    }
    let first = ((bounds.0 - origin) * delta.signum(), delta.abs());
    let last = ((bounds.1 - origin) * delta.signum(), delta.abs());
    let (first, last) = if less(first, last) {
        (first, last)
    } else {
        (last, first)
    };
    if less(interval.0, first) {
        interval.0 = first;
    }
    if less(last, interval.1) {
        interval.1 = last;
    }
    !less(interval.1, interval.0)
}

impl GameWorld {
    pub(in crate::game_world::movement_step) fn bridge_body_sweep_clear(
        &self,
        kind: UnitKind,
        from: WorldPosition,
        to: WorldPosition,
    ) -> bool {
        let Terrain::Map { overlay, .. } = &self.terrain else {
            return true;
        };
        if kind.is_naval()
            || (overlay.bridge_connections(from.tile_floor()).is_none()
                && overlay.bridge_connections(to.tile_floor()).is_none())
            || !self.bridge_water_clear(from.tile_floor())
            || !self.bridge_water_clear(to.tile_floor())
        {
            return true;
        }
        // Shore approaches also use the raster deck's terminal tile caps.
        // Existing body/tile checks cover those entries; the strip-union
        // proof applies once both endpoints lie on the continuous links.
        let radius = kind.terrain_radius();
        if !self.on_bridge_corridor(from, radius) || !self.on_bridge_corridor(to, radius) {
            return true;
        }
        let here = from.tile_floor();
        let margin = i128::from(512 - radius);
        let velocity = (i128::from(to.x - from.x), i128::from(to.y - from.y));
        let mut intervals = Vec::new();
        // A physical step is at most half a tile. Only neighbouring deck links
        // can support any part of it, even when the full bridge is very long.
        for y in -1..=1 {
            for x in -1..=1 {
                let tile = TileCoord::new(here.x + x, here.y + y);
                let Some(mask) = overlay.bridge_connections(tile) else {
                    continue;
                };
                let Ok(start) = WorldPosition::from_tile_center(tile) else {
                    continue;
                };
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        // A shore tile has no mask: its incoming link is
                        // stored only on the deck. Retain both directions.
                        if (dx == 0 && dy == 0)
                            || mask & (1 << ((dy + 1) * 3 + dx + 1)) == 0
                            || overlay.bridge_edge(tile, TileCoord::new(tile.x + dx, tile.y + dy))
                                != Some(true)
                        {
                            continue;
                        }
                        let (vx, vy) = (i128::from(dx * 1024), i128::from(dy * 1024));
                        let length = vx * vx + vy * vy;
                        let (px, py) = (i128::from(from.x - start.x), i128::from(from.y - start.y));
                        // Floor the square root to inset conservatively; all interval
                        // comparisons stay rational and never rely on sampled points.
                        let width = margin * length.isqrt();
                        let mut interval = ((0, 1), (1, 1));
                        if clip(
                            px * vx + py * vy,
                            velocity.0 * vx + velocity.1 * vy,
                            (0, length),
                            &mut interval,
                        ) && clip(
                            px * vy - py * vx,
                            velocity.0 * vy - velocity.1 * vx,
                            (-width, width),
                            &mut interval,
                        ) {
                            intervals.push(interval);
                        }
                    }
                }
            }
        }
        intervals.sort_unstable_by(|a, b| (a.0.0 * b.0.1).cmp(&(b.0.0 * a.0.1)));
        let mut covered = (0, 1);
        for (low, high) in intervals {
            if less(covered, low) {
                return false;
            }
            if less(covered, high) {
                covered = high;
            }
            if !less(covered, (1, 1)) {
                return true;
            }
        }
        false
    }
}
