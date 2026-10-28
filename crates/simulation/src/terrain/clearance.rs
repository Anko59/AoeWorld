//! Derived, lazy 64x64 clearance pages, independently keyed for land and water.
//! A tile centre adjacent to solid ground has one half tile of clearance.
//! The eight-tile halo makes capped distances exact even across page edges.
use crate::{GameWorld, UnitKind};
use aoe_core::{TileCoord, TileRect};
use std::collections::BTreeMap;

const SIDE: i32 = 64;
const HALO: i32 = 8;
const PADDED: usize = (SIDE + 2 * HALO) as usize;
const MAX_PAGES: usize = 4096;

#[derive(Debug, Default)]
pub(crate) struct Field {
    pages: BTreeMap<(bool, i32, i32), Box<[u8; 4096]>>,
}

impl Field {
    pub(crate) fn clear(&mut self) {
        self.pages.clear();
    }

    pub(crate) fn invalidate(&mut self, area: TileRect) {
        self.pages.retain(|&(_, x, y), _| {
            x * SIDE + SIDE + HALO <= area.min.x
                || y * SIDE + SIDE + HALO <= area.min.y
                || x * SIDE - HALO >= area.max.x
                || y * SIDE - HALO >= area.max.y
        });
    }
}

impl GameWorld {
    /// Chebyshev clearance from a tile centre to static obstacles, in half
    /// tiles, capped at 15. Impassable tiles have zero clearance. Bodies do not
    /// affect this field. A failed environment read returns zero without caching
    /// the failure; exact movement sweeps remain authoritative.
    pub fn static_clearance(&self, kind: UnitKind, tile: TileCoord) -> u8 {
        if tile.x < 0
            || tile.y < 0
            || tile.x >= self.config.width_tiles
            || tile.y >= self.config.height_tiles
        {
            return 0;
        }
        let key = (
            kind.is_naval(),
            tile.x.div_euclid(SIDE),
            tile.y.div_euclid(SIDE),
        );
        let at = (tile.y.rem_euclid(SIDE) * SIDE + tile.x.rem_euclid(SIDE)) as usize;
        let mut field = self
            .clearance
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(page) = field.pages.get(&key) {
            return page[at];
        }
        let mut distance = [8_u8; PADDED * PADDED];
        for y in 0..PADDED {
            for x in 0..PADDED {
                let p = TileCoord::new(
                    key.1 * SIDE + x as i32 - HALO,
                    key.2 * SIDE + y as i32 - HALO,
                );
                let passable = match self.unit_tile_passable(kind, p, &|| false) {
                    Ok(passable) => passable,
                    Err(_) => return 0,
                };
                if !passable || self.economy.blocked_tiles.contains(&p) {
                    distance[y * PADDED + x] = 0;
                }
            }
        }
        // Two chamfer passes give exact L-infinity distance to solid tiles.
        for y in 0..PADDED {
            for x in 0..PADDED {
                relax(&mut distance, x, y, -1);
            }
        }
        for y in (0..PADDED).rev() {
            for x in (0..PADDED).rev() {
                relax(&mut distance, x, y, 1);
            }
        }
        let mut page = Box::new([0; 4096]);
        for y in 0..SIDE as usize {
            for x in 0..SIDE as usize {
                let d = distance[(y + HALO as usize) * PADDED + x + HALO as usize];
                page[y * SIDE as usize + x] = (2 * d).saturating_sub(1).min(15);
            }
        }
        let result = page[at];
        // Cache eviction changes cost only, never authoritative state or hashes.
        if field.pages.len() == MAX_PAGES {
            field.pages.clear();
        }
        field.pages.insert(key, page);
        result
    }
}

fn relax(distance: &mut [u8], x: usize, y: usize, direction: isize) {
    let at = y * PADDED + x;
    for (dx, dy) in [
        (direction, 0),
        (-1, direction),
        (0, direction),
        (1, direction),
    ] {
        let (nx, ny) = (x as isize + dx, y as isize + dy);
        if (0..PADDED as isize).contains(&nx) && (0..PADDED as isize).contains(&ny) {
            distance[at] = distance[at].min(distance[ny as usize * PADDED + nx as usize] + 1);
        }
    }
}
