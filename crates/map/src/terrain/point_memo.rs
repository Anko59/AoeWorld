//! Two-way memo of composed single-tile results (terrain and resource).
//!
//! The composed landscape of an immutable generator never changes, yet movement
//! and route planning ask for the same tiles many times per second. Only
//! successful, uncancelled answers are stored, so page failures and
//! cancellation behave exactly as before.
use super::{ResourceNode, Tile};
use aoe_core::TileCoord;

pub(super) type Point = Option<(Tile, Option<ResourceNode>)>;

/// Fixed table size. The browser build links this crate too and walks chunks
/// rather than single tiles, so it keeps no memo at all: its memory stays flat.
#[cfg(target_arch = "wasm32")]
pub(super) const SLOTS: usize = 0;
#[cfg(not(target_arch = "wasm32"))]
pub(super) const SLOTS: usize = 1 << 17;

#[derive(Debug, Default)]
pub(super) struct PointMemo {
    slots: Vec<Option<(TileCoord, Point)>>,
    bases: Vec<Option<(TileCoord, Option<Tile>)>>,
}

fn index(tile: TileCoord) -> usize {
    let hash = (tile.x as u32 as u64).wrapping_mul(0x9E37_79B1_85EB_CA87)
        ^ (tile.y as u32 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    (((hash >> 20) as usize) & (SLOTS / 2).wrapping_sub(1)) * 2
}

fn lookup<T: Copy>(slots: &[Option<(TileCoord, T)>], tile: TileCoord) -> Option<T> {
    let at = index(tile);
    slots
        .get(at..at + 2)?
        .iter()
        .find_map(|entry| entry.and_then(|(key, value)| (key == tile).then_some(value)))
}

fn store<T: Copy>(slots: &mut Vec<Option<(TileCoord, T)>>, tile: TileCoord, value: T) {
    if slots.is_empty() {
        slots.resize(SLOTS, None);
    }
    let at = index(tile);
    let set = &mut slots[at..at + 2];
    if let Some(entry) = set
        .iter_mut()
        .find(|entry| entry.is_some_and(|(key, _)| key == tile))
    {
        *entry = Some((tile, value));
    } else {
        // Insertion order is sufficient to retain an alternating collision
        // pair. Both tables keep exactly their existing fixed slot budget.
        set[1] = set[0];
        set[0] = Some((tile, value));
    }
}

impl PointMemo {
    pub(super) fn get(&self, tile: TileCoord) -> Option<Point> {
        if SLOTS == 0 {
            return None;
        }
        lookup(&self.slots, tile)
    }

    pub(super) fn insert(&mut self, tile: TileCoord, point: Point) {
        if SLOTS == 0 {
            return;
        }
        store(&mut self.slots, tile, point);
    }

    /// The base physical tile used by route edge checks.
    pub(super) fn get_base(&self, tile: TileCoord) -> Option<Option<Tile>> {
        if SLOTS == 0 {
            return None;
        }
        lookup(&self.bases, tile)
    }

    pub(super) fn insert_base(&mut self, tile: TileCoord, base: Option<Tile>) {
        if SLOTS == 0 {
            return;
        }
        store(&mut self.bases, tile, base);
    }
}

#[cfg(test)]
mod tests;
