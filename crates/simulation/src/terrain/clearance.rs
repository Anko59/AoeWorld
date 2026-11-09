//! Derived, lazy 64x64 clearance pages, independently keyed for land and water.
//! A tile centre adjacent to solid ground has one half tile of clearance.
//! The eight-tile halo makes capped distances exact even across page edges.
//! A changed area re-reads only its own tiles of a page's retained solid set.
use crate::{GameWorld, UnitKind};
use aoe_core::{TileCoord, TileRect};
use std::collections::BTreeMap;

const SIDE: i32 = 64;
const HALO: i32 = 8;
const PADDED: usize = (SIDE + 2 * HALO) as usize;
// Page `k` covers tiles `64k + 8 .. 64k + 72` and reads `64k .. 64k + 80`:
// that padded window spans three 32-tile walk chunks a side, not four.
const MAX_PAGES: usize = 4096;

#[derive(Debug, Default)]
pub(crate) struct Field {
    pages: BTreeMap<(bool, i32, i32), Box<[u8; 4096]>>,
    /// The solid tiles each page was derived from, kept across invalidation:
    /// a rebuild reads only the tiles of the areas changed since.
    solids: BTreeMap<(bool, i32, i32), Box<Solid>>,
}

#[derive(Debug)]
struct Solid {
    bits: [u64; WORDS],
    /// Tiles inside changed areas since `bits` was read.
    dirty: [u64; WORDS],
}

const WORDS: usize = (PADDED * PADDED).div_ceil(64);

/// The padded page's tile range covered by `min..max` along one axis.
fn span(min: i32, max: i32, origin: i32) -> std::ops::Range<usize> {
    (min - origin).clamp(0, PADDED as i32) as usize..(max - origin).clamp(0, PADDED as i32) as usize
}

fn overlaps(x: i32, y: i32, area: TileRect) -> bool {
    let padded = PADDED as i32;
    !(x * SIDE + padded <= area.min.x
        || y * SIDE + padded <= area.min.y
        || x * SIDE >= area.max.x
        || y * SIDE >= area.max.y)
}

impl Field {
    pub(crate) fn clear(&mut self) {
        self.pages.clear();
        self.solids.clear();
    }

    pub(crate) fn invalidate(&mut self, area: TileRect) {
        self.pages.retain(|&(_, x, y), _| !overlaps(x, y, area));
        for (&(_, x, y), solid) in &mut self.solids {
            if !overlaps(x, y, area) {
                continue;
            }
            let (left, top) = (x * SIDE, y * SIDE);
            for row in span(area.min.y, area.max.y, top) {
                for column in span(area.min.x, area.max.x, left) {
                    let index = row * PADDED + column;
                    solid.dirty[index / 64] |= 1 << (index % 64);
                }
            }
        }
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
            (tile.x - HALO).div_euclid(SIDE),
            (tile.y - HALO).div_euclid(SIDE),
        );
        let at =
            ((tile.y - HALO).rem_euclid(SIDE) * SIDE + (tile.x - HALO).rem_euclid(SIDE)) as usize;
        let prior = {
            let mut field = self
                .clearance
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if let Some(page) = field.pages.get(&key) {
                return page[at];
            }
            field.solids.remove(&key)
        };
        // Built without the lock: other readers keep using resident pages. Two
        // readers of one missing page derive the same values from the same world.
        let Some(solid) = self.clearance_solid(kind, key, prior) else {
            return 0;
        };
        let page = derive_page(&solid.bits);
        let result = page[at];
        let mut field = self
            .clearance
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // Cache eviction changes cost only, never authoritative state or hashes.
        // Retained solid sets outlive invalidated pages: bound them too.
        if field.pages.len().max(field.solids.len() / 2) >= MAX_PAGES {
            field.pages.clear();
            field.solids.clear();
        }
        field.pages.insert(key, page);
        field.solids.insert(key, solid);
        result
    }

    /// The padded page's solid tiles: only the changed areas of a retained
    /// set, otherwise every tile. `None` when the environment cannot be read.
    fn clearance_solid(
        &self,
        kind: UnitKind,
        key: (bool, i32, i32),
        prior: Option<Box<Solid>>,
    ) -> Option<Box<Solid>> {
        let (left, top) = (key.1 * SIDE, key.2 * SIDE);
        let read = |solid: &mut Solid, x: usize, y: usize| -> Option<()> {
            let p = TileCoord::new(left + x as i32, top + y as i32);
            let passable = self.unit_tile_passable(kind, p, &|| false).ok()?;
            let index = y * PADDED + x;
            let bit = 1 << (index % 64);
            if !passable || self.economy.blocked_tiles.contains(&p) {
                solid.bits[index / 64] |= bit;
            } else {
                solid.bits[index / 64] &= !bit;
            }
            Some(())
        };
        if let Some(mut solid) = prior {
            let dirty = std::mem::replace(&mut solid.dirty, [0; WORDS]);
            for (word, mut bits) in dirty.into_iter().enumerate() {
                while bits != 0 {
                    let index = word * 64 + bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    read(&mut solid, index % PADDED, index / PADDED)?;
                }
            }
            return Some(solid);
        }
        self.prepare_clearance_page(key.1, key.2);
        let mut solid = Box::new(Solid {
            bits: [0; WORDS],
            dirty: [0; WORDS],
        });
        for y in 0..PADDED {
            for x in 0..PADDED {
                read(&mut solid, x, y)?;
            }
        }
        Some(solid)
    }

    /// Compose the walk chunks under a padded page before reading its 6,400
    /// tiles: a cold point read evaluates the landscape alone (about 85,000
    /// instructions a tile), a composed chunk shares that work. Chunks are a
    /// pure function of the generator, so this changes cost, never a value.
    fn prepare_clearance_page(&self, page_x: i32, page_y: i32) {
        let super::Terrain::Map { generator, .. } = &self.terrain else {
            return;
        };
        let chunks = |page: i32, limit: i32| {
            let last = (limit - 1).div_euclid(aoe_map::CHUNK_TILES);
            let low = (page * SIDE).div_euclid(aoe_map::CHUNK_TILES).max(0);
            let high = (page * SIDE + PADDED as i32 - 1)
                .div_euclid(aoe_map::CHUNK_TILES)
                .min(last);
            low..=high
        };
        for y in chunks(page_y, self.config.height_tiles) {
            for x in chunks(page_x, self.config.width_tiles) {
                // A failed composition leaves the point reads to report it.
                let _ = generator.precompose_walk_chunk(x, y, &|| false);
            }
        }
    }
}

/// Two chamfer passes give exact L-infinity distance to solid tiles.
fn derive_page(solid: &[u64; WORDS]) -> Box<[u8; 4096]> {
    let mut distance = [8_u8; PADDED * PADDED];
    for (index, d) in distance.iter_mut().enumerate() {
        if solid[index / 64] >> (index % 64) & 1 == 1 {
            *d = 0;
        }
    }
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
    page
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
