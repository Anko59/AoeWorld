//! Direct-mapped memo of composed single-tile results (terrain and resource).
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
    ((hash >> 20) as usize) & SLOTS.wrapping_sub(1)
}

impl PointMemo {
    pub(super) fn get(&self, tile: TileCoord) -> Option<Point> {
        if SLOTS == 0 {
            return None;
        }
        match self.slots.get(index(tile)) {
            Some(Some((key, point))) if *key == tile => Some(*point),
            _ => None,
        }
    }

    pub(super) fn insert(&mut self, tile: TileCoord, point: Point) {
        if SLOTS == 0 {
            return;
        }
        if self.slots.is_empty() {
            self.slots = vec![None; SLOTS];
        }
        self.slots[index(tile)] = Some((tile, point));
    }

    /// The base physical tile used by route edge checks.
    pub(super) fn get_base(&self, tile: TileCoord) -> Option<Option<Tile>> {
        if SLOTS == 0 {
            return None;
        }
        match self.bases.get(index(tile)) {
            Some(Some((key, base))) if *key == tile => Some(*base),
            _ => None,
        }
    }

    pub(super) fn insert_base(&mut self, tile: TileCoord, base: Option<Tile>) {
        if SLOTS == 0 {
            return;
        }
        if self.bases.is_empty() {
            self.bases = vec![None; SLOTS];
        }
        self.bases[index(tile)] = Some((tile, base));
    }
}

#[cfg(test)]
mod tests;
